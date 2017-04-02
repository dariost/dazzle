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
use rand::{Rng, StdRng};
use rand::distributions::{IndependentSample, Range};
use serde::ser::Serialize;
use serde_json;
use std::collections::{HashMap, HashSet, VecDeque};
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
}

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

pub struct Game
{
    grid: Vec<Vec<Option<u64>>>,
    tokens: HashSet<Point>,
    turns_left: u64,
    rng: StdRng,
    gen_col: Range<usize>,
    gen_row: Range<usize>,
    game_id: u64,
    players: HashMap<u64, Player>,
    moved: HashMap<u64, bool>,
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
}

impl Game
{
    pub fn new(players: HashMap<u64, Player>, total_turns: u64, game_id: u64) -> Game
    {
        let mut players = players;
        let num_players = players.len();
        let rows = 6 + (num_players as f64).sqrt().round() as usize;
        let cols = rows * 2;
        let grid: Vec<Vec<Option<u64>>> = vec![vec![None; cols]; rows];
        let mut rng = StdRng::new().unwrap();
        let gen_row = Range::new(0, rows);
        let gen_col = Range::new(0, cols);
        let mut moved: HashMap<u64, bool> = Default::default();
        for player in players.values_mut()
        {
            player.position = Point {
                x: gen_col.ind_sample(&mut rng),
                y: gen_row.ind_sample(&mut rng),
            };
            moved.insert(player.id, false);
        }
        Game {
            grid: grid,
            tokens: Default::default(),
            turns_left: total_turns,
            rng: rng,
            gen_col: gen_col,
            gen_row: gen_row,
            game_id: game_id,
            players: players,
            moved: moved,
        }
    }

    pub fn finished(&self) -> bool
    {
        self.turns_left == 0
    }

    pub fn tick(&mut self)
    {
        for player in self.players.values()
        {
            self.grid[player.position.y][player.position.x] = Some(player.id);
        }
        let mut visited: HashSet<Point> = Default::default();
        let cols = self.grid[0].len();
        let rows = self.grid.len();
        for y in 0..rows
        {
            for x in 0..cols
            {
                if visited.contains(&Point { x: x, y: y }) || self.grid[y][x].is_some()
                {
                    continue;
                }
                let mut to_fill: HashSet<Point> = Default::default();
                let mut colors: HashSet<u64> = Default::default();
                let mut queue: VecDeque<Point> = Default::default();
                let mut valid = true;
                queue.push_back(Point { x: x, y: y });
                while !queue.is_empty()
                {
                    let p = queue.pop_front().unwrap();
                    if let Some(color) = self.grid[p.y][p.x]
                    {
                        colors.insert(color);
                        continue;
                    }
                    if to_fill.contains(&p)
                    {
                        continue;
                    }
                    to_fill.insert(p.clone());
                    visited.insert(p.clone());
                    if p.x + 1 >= cols || (p.x as i64) - 1 < 0 || p.y + 1 >= rows || (p.y as i64) - 1 < 0
                    {
                        valid = false;
                        break;
                    }
                    queue.push_back(Point {
                                        x: p.x + 1,
                                        y: p.y,
                                    });
                    queue.push_back(Point {
                                        x: p.x - 1,
                                        y: p.y,
                                    });
                    queue.push_back(Point {
                                        x: p.x,
                                        y: p.y + 1,
                                    });
                    queue.push_back(Point {
                                        x: p.x,
                                        y: p.y - 1,
                                    });
                }
                if valid && colors.len() == 1
                {
                    let color = colors.drain().last().unwrap();
                    for p in to_fill.drain()
                    {
                        self.grid[p.y][p.x] = Some(color);
                    }
                }
            }
        }
        for player in self.players.values_mut()
        {
            *self.moved.get_mut(&player.id).unwrap() = false;
            if self.tokens.contains(&player.position)
            {
                let mut count: u64 = 0;
                self.tokens.remove(&player.position);
                for y in 0..rows
                {
                    for x in 0..cols
                    {
                        match self.grid[y][x]
                        {
                            Some(c) if c == player.id =>
                            {
                                self.grid[y][x] = None;
                                count += 1;
                            }
                            _ => continue,
                        }
                    }
                }
                player.points += count;
            }
        }
        self.turns_left -= 1;
        let mut count = 0;
        while self.rng.next_f64() < 0.1
        {
            count += 1;
        }
        for _ in 0..count
        {
            self.tokens.insert(Point {
                                   x: self.gen_col.ind_sample(&mut self.rng),
                                   y: self.gen_row.ind_sample(&mut self.rng),
                               });
        }
    }

    pub fn action(&mut self, id: u64, command: ClientCommand) -> Result<(), String>
    {
        let ok = match self.moved.get(&id)
        {
            Some(x) => !x,
            None => false,
        };
        if !ok
        {
            return Err(String::from("Already moved"));
        }
        *self.moved.get_mut(&id).unwrap() = true;
        let mut positions: HashSet<Point> = Default::default();
        for pos in self.players.values().map(|p| p.position.clone())
        {
            positions.insert(pos);
        }
        let player = self.players.get_mut(&id).unwrap();
        let cols = self.grid[0].len();
        let rows = self.grid.len();
        match command
        {
            ClientCommand::Nothing => Ok(()),
            ClientCommand::Move(direction) =>
            {
                match direction
                {
                    Direction::Up if player.position.y + 1 < rows &&
                                     !positions.contains(&Point {
                                                              x: player.position.x,
                                                              y: player.position.y + 1,
                                                          }) =>
                    {
                        player.position.y += 1;
                        Ok(())
                    }
                    Direction::Down if (player.position.y as i64) - 1 >= 0 &&
                                       !positions.contains(&Point {
                                                                x: player.position.x,
                                                                y: player.position.y - 1,
                                                            }) =>
                    {
                        player.position.y -= 1;
                        Ok(())
                    }
                    Direction::Right if player.position.x + 1 < cols &&
                                        !positions.contains(&Point {
                                                                 x: player.position.x + 1,
                                                                 y: player.position.y,
                                                             }) =>
                    {
                        player.position.x += 1;
                        Ok(())
                    }
                    Direction::Left if (player.position.x as i64) - 1 >= 0 &&
                                       !positions.contains(&Point {
                                                                x: player.position.x - 1,
                                                                y: player.position.y,
                                                            }) =>
                    {
                        player.position.x -= 1;
                        Ok(())
                    }
                    _ =>
                    {
                        *self.moved.get_mut(&id).unwrap() = false;
                        Err(String::from("Moved out of grid or in a cell already taken"))
                    }
                }
            }

        }
    }
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
                Err(Error::ConnectionClosed) =>
                {
                    sender.send(MessageResponse::Disconnected(seed)).unwrap();
                    break;
                }
                Err(_) => continue,
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
        }
    }

    pub fn send_overview(&mut self)
    {
        unimplemented!();
    }

    pub fn main(&mut self)
    {
        while let Ok(x) = self.incoming_connections.try_recv()
        {
            self.connections.insert(self.seed, Connection::new(x, self.message_sender.clone(), self.seed));
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
            self.game_start_ticks_left -= 1;
        }
        else if self.game.is_none() && self.game_start_ticks_left == 0 && self.queue.len() >= 2
        {
            let mut players: HashMap<u64, Player> = Default::default();
            for (player_id, player_value) in self.queue.drain()
            {
                players.insert(player_id, player_value);
            }
            self.game = Some(Game::new(players, self.game_turns, self.game_id));
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
            let mut to_readd: Vec<(u64, ClientRole)> = Vec::new();
            for (key, player) in self.game
                    .as_ref()
                    .unwrap()
                    .players
                    .iter()
            {
                to_readd.push((key.clone(), ClientRole::Player(PlayerInfo { name: player.name.clone() })));
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
                }
            }
            ClientRole::Player(info) =>
            {
                let user_game_id = player_hash(&info);
                if self.queue.contains_key(&user_game_id)
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
                                              name: info.name,
                                              id: user_game_id,
                                              points: 0,
                                              position: Point { x: 0, y: 0 },
                                          });
                        if self.game.is_none()
                        {
                            self.game_start_ticks_left = self.game_start_ticks;
                        }
                    }
                    self.send_data(id, &ServerResponse::Ok);
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
        match self.connections
                  .get(&id)
                  .unwrap()
                  .role
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
        }
    }
}
