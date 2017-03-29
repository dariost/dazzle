/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 *
 * Copyright 2017 - Dario Ostuni <dario.ostuni@gmail.com>
 *
 */

use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::thread::JoinHandle;
use tungstenite::{Error, Message, WebSocket, accept};
use tungstenite::protocol::Role;

enum ConnectionType
{
    Viewer,
    InGame,
    InQueue,
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
    Mail(Message),
    Disconnected(u64),
}

pub struct Server
{
    connections: HashMap<u64, Connection>,
    incoming_connections: Receiver<WebSocket<TcpStream>>,
    incoming_messages: Receiver<MessageResponse>,
    message_sender: Sender<MessageResponse>,
    seed: u64,
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
                        Ok(x) => sender.send(MessageResponse::Mail(x)).unwrap(),
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
    pub fn new() -> Server
    {
        let (sen, rec) = channel();
        thread::spawn(move || {
            let listener = TcpListener::bind("0.0.0.0:42000").unwrap();
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
        }
    }

    pub fn main(&mut self)
    {
        while let Ok(x) = self.incoming_connections.try_recv()
        {
            self.connections.insert(self.seed, Connection::new(x, self.message_sender.clone(), self.seed));
            self.seed += 1;
        }
    }
}
