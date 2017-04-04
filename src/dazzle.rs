/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 *
 * Copyright 2017 - Dario Ostuni <dario.ostuni@gmail.com>
 *
 */

#[macro_use]
extern crate log;
extern crate mowl;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate twox_hash;
extern crate tungstenite;
extern crate serde;
extern crate rand;
extern crate url;

mod common;

use common::*;
use serde::Serialize;
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::io::ErrorKind;
use std::process::{Command, Stdio};
use tungstenite::{Error, Message, WebSocket};
use tungstenite::client::connect;
use url::Url;

fn send_data<T: ?Sized + Serialize, U: Read + Write>(ws: &mut WebSocket<U>, value: &T)
{
    match serde_json::to_string(value)
    {
        Ok(s) =>
        {
            match ws.write_message(Message::text(s))
            {
                Ok(_) =>
                {}
                Err(Error::Io(x)) =>
                {
                    match x.kind()
                    {
                        ErrorKind::WouldBlock =>
                        {
                            match ws.write_pending()
                            {
                                Ok(_) =>
                                {}
                                Err(why) => error!("Unable to flush WebSocket {}", why),
                            }
                        }
                        _ => error!("I/O error"),
                    }
                }
                Err(Error::ConnectionClosed) => panic!("Connection closed!"),
                Err(_) => error!("Error while sending data"),
            }
        }
        Err(_) => unreachable!(),
    };
}

#[cfg_attr(feature = "cargo-clippy", allow(many_single_char_names))]
fn main()
{
    mowl::init_with_level(log::LogLevel::Info).unwrap();
    info!("Starting dazzle...");
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2
    {
        error!("Usage: dazzle ws://ip:port/ <program> [arguments]");
        return;
    }
    info!("dazzle started successfully!");
    let mut websocket = match connect(Url::parse(args[0].as_str()).unwrap())
    {
        Ok(ws) => ws,
        Err(why) => panic!("Cannot connect to {} -> {}", args[0], why),
    };
    info!("Connected to {}", args[0]);
    let child = Command::new(&args[1])
        .args(args.iter().skip(2))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Cannot fork");
    info!("Started guest program");
    let mut stdout = BufReader::new(child.stdout.unwrap());
    let mut stdin = child.stdin.unwrap();
    let mut name_string = String::new();
    stdout.read_line(&mut name_string).unwrap();
    let connect_message = ClientMessage::HandShake(ClientRole::Player(PlayerInfo { name: name_string }));
    send_data(&mut websocket, &connect_message);
    let response: ServerResponse = serde_json::from_str(websocket.read_message()
                                                            .unwrap()
                                                            .to_text()
                                                            .unwrap())
            .unwrap();
    if let ServerResponse::Error(s) = response
    {
        panic!("Error from server: {}", s);
    }
    loop
    {
        let overview: Overview = serde_json::from_str(websocket.read_message()
                                                          .unwrap()
                                                          .to_text()
                                                          .unwrap())
                .unwrap();
        let n = overview.players.len();
        let r = overview.grid.len();
        let c = overview.grid[0].len();
        let t = overview.tokens.len();
        let e = overview.turns_left;
        let m = overview.ms_for_turn;
        writeln!(&mut stdin, "{} {} {} {} {} {}", n, r, c, t, e, m).unwrap();
        for p in &overview.players
        {
            writeln!(&mut stdin, "{} {} {} {} {}", p.id, p.name, p.points, p.position.x, p.position.y).unwrap();
        }
        for rows in &overview.grid
        {
            for pos in 0..rows.len()
            {
                if let Some(value) = rows[pos]
                {
                    write!(&mut stdin, "{}", value).unwrap();
                }
                else
                {
                    write!(&mut stdin, "{}", -1).unwrap();
                }
                if pos != rows.len() - 1
                {
                    write!(&mut stdin, " ").unwrap();
                }
            }
            writeln!(&mut stdin, "").unwrap();
        }
        for token in &overview.tokens
        {
            writeln!(&mut stdin, "{} {}", token.x, token.y).unwrap();
        }
        stdin.flush().unwrap();
        let mut cli = String::new();
        stdout.read_line(&mut cli).unwrap();
        let command = match cli.trim()
        {
            "NOTHING" => ClientCommand::Nothing,
            "UP" => ClientCommand::Move(Direction::Up),
            "DOWN" => ClientCommand::Move(Direction::Down),
            "LEFT" => ClientCommand::Move(Direction::Left),
            "RIGHT" => ClientCommand::Move(Direction::Right),
            "QUIT" => break,
            cmd => panic!("Invalid command: {}", cmd),
        };
        let command = ClientMessage::Command(command);
        send_data(&mut websocket, &command);
        let _: ServerResponse = serde_json::from_str(websocket.read_message()
                                                         .unwrap()
                                                         .to_text()
                                                         .unwrap())
                .unwrap();
    }
    websocket.close().unwrap();
}

/* INPUT FORMAT
N: number of players
R, C: rows and columns of the grid
T: number of tokens
E: turns left
M: milliseconds to make the move
positionals arguments
ID: player id
S: player name (string)
P: points
X, Y: position

INPUT:
N R C T E M
N lines containing: ID S P X Y
R lines, each containing C numbers: -1 for nothing, the player id otherwise
T lines: X Y
*/
