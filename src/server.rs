/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 *
 * Copyright 2017 - Dario Ostuni <dario.ostuni@gmail.com>
 *
 */

use chasher::player_hash;
use common::*;
use game::Game;
use serde::ser::Serialize;
use serde_json;
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;
use tungstenite::{Error, Message, WebSocket, accept};
use tungstenite::protocol::Role;

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerConfig
{
    tick_time_ms: u64,
    server_port: u16,
    game_start_ticks: u64,
    game_turns: u64,
    token_rate: f64,
}

#[derive(PartialEq, Eq, Hash, Clone)]
enum ConnectionType
{
    Viewer,
    Player(u64),
    Unknown,
}

struct Connection
{
    role: ConnectionType,
    socket: WebSocket<TcpStream>,
}

enum MessageResponse
{
    Mail(u64, Message),
    Disconnected(u64),
}

pub struct Server
{
    connections: HashMap<u64, Connection>,
    incoming_connections: Receiver<WebSocket<TcpStream>>,
    incoming_messages: Receiver<MessageResponse>,
    message_sender: Sender<MessageResponse>,
    queue: HashMap<u64, Player>,
    seed: u64,
    tick_time: u64,
    game_id: u64,
    game_start_ticks: u64,
    game_turns: u64,
    game_start_ticks_left: u64,
    game: Option<Game>,
    token_rate: f64,
}

impl Drop for Connection
{
    fn drop(&mut self)
    {
        let _ = self.socket.close();
    }
}

impl Connection
{
    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub fn new(ws: WebSocket<TcpStream>, sender: Sender<MessageResponse>, seed: u64) -> Connection
    {
        let mut ws = ws;
        let conn = Connection {
            socket: WebSocket::from_raw_socket(ws.get_ref().try_clone().unwrap(), Role::Server),
            role: ConnectionType::Unknown,
        };
        thread::spawn(move || loop
        {
            match ws.read_message()
            {
                Ok(x) => sender.send(MessageResponse::Mail(seed, x)).unwrap(),
                Err(_) =>
                {
                    sender.send(MessageResponse::Disconnected(seed)).unwrap();
                    break;
                }
            }
        });
        conn
    }
}

impl Server
{
    pub fn new(config: ServerConfig) -> Server
    {
        let (sen, rec) = channel();
        let server_port = config.server_port;
        thread::spawn(move || {
            let listener = TcpListener::bind(("0.0.0.0", server_port)).unwrap();
            for stream in listener.incoming()
            {
                match stream
                {
                    Ok(s) =>
                    {
                        match accept(s)
                        {
                            Ok(w) => sen.send(w).unwrap(),
                            Err(_) => error!("Error while accepting WebSocket connection"),
                        }
                    }
                    Err(why) => error!("Error while accepting connection: {}", why),
                }
            }
        });
        let (sen_msg, rec_msg) = channel();
        Server {
            connections: Default::default(),
            incoming_connections: rec,
            incoming_messages: rec_msg,
            message_sender: sen_msg,
            seed: 0,
            tick_time: config.tick_time_ms,
            game_id: 0,
            queue: Default::default(),
            game_start_ticks: config.game_start_ticks,
            game_turns: config.game_turns,
            game_start_ticks_left: config.game_turns,
            game: None,
            token_rate: config.token_rate,
        }
    }

    pub fn send_overview(&mut self)
    {
        if self.game.is_none()
        {
            error!("Trying to generate overview while there is no game");
            return;
        }
        let overview: Overview;
        let mut gaming_ids: HashSet<u64> = Default::default();
        {
            let game = self.game.as_ref().unwrap();
            let mut tokens = game.tokens.clone();
            let mut players = game.players.clone();
            let tokens = tokens.drain().collect();
            let players = players.drain().map(|(_, x)| x).collect();
            overview = Overview {
                game_id: game.game_id,
                turns_left: game.turns_left,
                ms_for_turn: self.tick_time,
                grid: game.grid.clone(),
                tokens: tokens,
                players: players,
            };
            for v in game.players.keys()
            {
                gaming_ids.insert(*v);
            }
        }
        let mut conn_info: HashSet<(u64, ConnectionType)> = Default::default();
        for (id, conn) in &self.connections
        {
            conn_info.insert((*id, conn.role.clone()));
        }
        for (id, role) in conn_info.drain()
        {
            match role
            {
                ConnectionType::Viewer => self.send_data(id, &overview),
                ConnectionType::Player(user_game_id) if gaming_ids.contains(&user_game_id) => self.send_data(id, &overview),
                _ => continue,
            }
        }
    }

    pub fn main(&mut self)
    {
        while let Ok(x) = self.incoming_connections.try_recv()
        {
            self.connections.insert(self.seed, Connection::new(x, self.message_sender.clone(), self.seed));
            info!("Accepted connection #{}", self.seed);
            self.seed += 1;
        }
        while let Ok(x) = self.incoming_messages.try_recv()
        {
            if let MessageResponse::Disconnected(id) = x
            {
                self.queue.remove(&id);
                if self.game.is_some()
                {
                    self.game
                        .as_mut()
                        .unwrap()
                        .players
                        .remove(&id);
                }
                self.connections.remove(&id);
                info!("Closed connection #{}", id);
            }
            else if let MessageResponse::Mail(id, msg) = x
            {
                let msg = match msg.into_text()
                {
                    Ok(s) => s,
                    Err(why) =>
                    {
                        error!("Received garbage: {}", why);
                        continue;
                    }
                };
                let msg: ClientMessage = match serde_json::from_str(msg.as_str())
                {
                    Ok(s) => s,
                    Err(why) =>
                    {
                        error!("Received garbage: {}", why);
                        continue;
                    }
                };
                if let ClientMessage::HandShake(role) = msg
                {
                    self.handle_accept(id, role, false);
                }
                else if let ClientMessage::Command(command) = msg
                {
                    self.handle_command(id, command);
                }
                else
                {
                    unreachable!();
                }
            }
            else
            {
                unreachable!();
            }
        }
        if self.game.is_none() && self.game_start_ticks_left > 0 && self.queue.len() >= 2
        {
            info!("Game staring in {} ticks", self.game_start_ticks_left);
            self.game_start_ticks_left -= 1;
        }
        else if self.game.is_none() && self.game_start_ticks_left == 0 && self.queue.len() >= 2
        {
            info!("Game started with {} players", self.queue.len());
            let mut players: HashMap<u64, Player> = Default::default();
            for (player_id, player_value) in self.queue.drain()
            {
                players.insert(player_id, player_value);
            }
            self.game = Some(Game::new(players, self.game_turns, self.game_id, self.token_rate));
            self.game
                .as_mut()
                .unwrap()
                .tick();
            self.send_overview();
            self.game_id += 1;
        }
        else if self.game.is_some() &&
                  !self.game
                       .as_ref()
                       .unwrap()
                       .finished()
        {
            self.game
                .as_mut()
                .unwrap()
                .tick();
            self.send_overview();
        }
        else if self.game.is_some() &&
                  self.game
                      .as_ref()
                      .unwrap()
                      .finished()
        {
            info!("Game ended!");
            let mut to_readd: Vec<(u64, ClientRole)> = Vec::new();
            for (key, player) in &self.game
                                      .as_ref()
                                      .unwrap()
                                      .players
            {
                to_readd.push((*key, ClientRole::Player(PlayerInfo { name: player.name.clone() })));
            }
            self.game = None;
            self.game_start_ticks_left = self.game_start_ticks;
            for player in to_readd
            {
                self.handle_accept(player.0, player.1, true);
            }
        }
        thread::sleep(Duration::from_millis(self.tick_time));
    }

    #[cfg_attr(feature = "cargo-clippy", allow(map_entry))]
    fn handle_accept(&mut self, id: u64, role: ClientRole, not_interactive: bool)
    {
        match role
        {
            ClientRole::Viewer =>
            {
                {
                    let v = self.connections.get_mut(&id);
                    if v.is_none()
                    {
                        return;
                    }
                    let v = v.unwrap();
                    v.role = ConnectionType::Viewer;
                }
                if !not_interactive
                {
                    self.send_data(id, &ServerResponse::Ok);
                    info!("Viewer connected");
                }
            }
            ClientRole::Player(info) =>
            {
                let mut info = info;
                info.name = String::from(info.name.trim());
                let user_game_id = player_hash(&info);
                if self.queue.contains_key(&user_game_id) || "\n\r\t ".chars().any(|x| info.name.contains(x))
                {
                    if !not_interactive
                    {
                        self.send_data(id, &ServerResponse::Error(String::from("Username already taken")));
                    }
                }
                else
                {
                    {
                        let conn = self.connections.get_mut(&id);
                        if conn.is_none()
                        {
                            error!("Trying to register a non-existent WebSocket as player!");
                            return;
                        }
                        let conn = conn.unwrap();
                        conn.role = ConnectionType::Player(user_game_id);
                        self.queue.insert(user_game_id,
                                          Player {
                                              name: info.name.clone(),
                                              id: user_game_id,
                                              points: 0,
                                              position: Point { x: 0, y: 0 },
                                          });
                        if self.game.is_none()
                        {
                            self.game_start_ticks_left = self.game_start_ticks;
                        }
                    }
                    if !not_interactive
                    {
                        info!("Player connected: {}", info.name);
                        self.send_data(id, &ServerResponse::Ok);
                    }
                }
            }
        };
    }

    fn handle_command(&mut self, id: u64, command: ClientCommand)
    {
        if !self.connections.contains_key(&id)
        {
            error!("Wrong ID is trying to execute command");
            return;
        }
        if self.game.is_none()
        {
            self.send_data(id, &ServerResponse::Error(String::from("No active game")));
            return;
        }
        match self.connections[&id].role
        {
            ConnectionType::Player(user_game_id) =>
            {
                match self.game
                          .as_mut()
                          .unwrap()
                          .action(user_game_id, command)
                {
                    Ok(_) => self.send_data(id, &ServerResponse::Ok),
                    Err(s) => self.send_data(id, &ServerResponse::Error(s)),
                }
            }
            _ =>
            {
                self.send_data(id, &ServerResponse::Error(String::from("Operation not allowed")));
            }
        }
    }

    fn send_data<T: ?Sized + Serialize>(&mut self, id: u64, value: &T)
    {
        match serde_json::to_string(value)
        {
            Ok(s) =>
            {
                match self.connections.get_mut(&id)
                {
                    Some(conn) =>
                    {
                        match conn.socket.write_message(Message::text(s))
                        {
                            Ok(_) =>
                            {}
                            Err(Error::Io(x)) =>
                            {
                                match x.kind()
                                {
                                    ErrorKind::WouldBlock =>
                                    {
                                        match conn.socket.write_pending()
                                        {
                                            Ok(_) =>
                                            {}
                                            Err(why) => error!("Unable to flush WebSocket {}", why),
                                        }
                                    }
                                    _ => error!("I/O error"),
                                }
                            }
                            Err(Error::ConnectionClosed) => self.message_sender.send(MessageResponse::Disconnected(id)).unwrap(),
                            Err(_) => error!("Error while sending data"),
                        }
                    }
                    None => error!("Missing id"),
                }
            }
            Err(_) => unreachable!(),
        };
    }
}

impl ServerConfig
{
    pub fn new() -> ServerConfig
    {
        ServerConfig {
            tick_time_ms: 500,
            server_port: 42000,
            game_start_ticks: 60,
            game_turns: 300,
            token_rate: 2.5,
        }
    }
}
