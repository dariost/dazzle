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
use serde::ser::Serialize;
use serde_json;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use tungstenite::{Error, Message, WebSocket, accept};
use tungstenite::protocol::Role;

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerConfig
{
    tick_time_ms: u64,
    server_port: u16,
    game_start_ticks: u64,
}

enum ConnectionType
{
    Viewer,
    InGame(u64),
    InQueue(u64),
    Unknown,
}

struct Connection
{
    role: ConnectionType,
    handle: JoinHandle<()>,
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
    players: HashMap<u64, Player>,
    queue: HashMap<u64, Player>,
    seed: u64,
    tick_time: u64,
    game_id: u64,
    in_game: bool,
    game_start_ticks: u64,
}

impl Connection
{
    #[cfg_attr(rustfmt, rustfmt_skip)]
    pub fn new(ws: WebSocket<TcpStream>, sender: Sender<MessageResponse>, seed: u64) -> Connection
    {
        let mut ws = ws;
        Connection {
            socket: WebSocket::from_raw_socket(ws.get_ref().try_clone().unwrap(), Role::Server),
            role: ConnectionType::Unknown,
            handle: thread::spawn(move ||
                loop
                {
                    match ws.read_message()
                    {
                        Ok(x) => sender.send(MessageResponse::Mail(seed, x)).unwrap(),
                        Err(Error::ConnectionClosed) => {
                            sender.send(MessageResponse::Disconnected(seed)).unwrap();
                            break;
                        },
                        Err(_) => continue,
                    }
                }),
        }
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
            in_game: false,
            players: Default::default(),
            queue: Default::default(),
            game_start_ticks: config.game_start_ticks,
        }
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
                    self.handle_accept(id, role);
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
        thread::sleep(Duration::from_millis(self.tick_time));
    }

    #[cfg_attr(feature = "cargo-clippy", allow(map_entry))]
    fn handle_accept(&mut self, id: u64, role: ClientRole)
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
                self.send_data(id, &ServerResponse::Ok);
            }
            ClientRole::Player(info) =>
            {
                let user_game_id = player_hash(&info);
                if self.queue.contains_key(&user_game_id)
                {
                    self.send_data(id, &ServerResponse::Error(String::from("Username already taken")));
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
                        conn.role = ConnectionType::InQueue(user_game_id);
                        self.queue.insert(user_game_id,
                                          Player {
                                              name: info.name,
                                              id: user_game_id,
                                              points: 0,
                                              position: Point { x: 0, y: 0 },
                                          });
                    }
                    self.send_data(id, &ServerResponse::Ok);
                }
            }
        };
    }

    fn handle_command(&mut self, id: u64, command: ClientCommand)
    {
        unimplemented!();
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
            tick_time_ms: 1000,
            server_port: 42000,
            game_start_ticks: 30,
        }
    }
}
