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

mod chasher;
mod common;
mod server;
mod game;

use server::Server;
use server::ServerConfig;
use std::env;
use std::fs::File;

fn try_open_config(custom: Option<String>) -> Option<File>
{
    let mut file_list = vec![String::from("dazzled.json"), String::from("/etc/dazzled.json")];
    if let Some(p) = custom
    {
        file_list.insert(0, p);
    }
    for file_name in &file_list
    {
        let f = File::open(file_name);
        if f.is_ok()
        {
            return f.ok();
        }
    }
    None
}

fn main()
{
    mowl::init_with_level(log::LogLevel::Info).unwrap();
    info!("Starting dazzled...");
    let args: Vec<String> = env::args().collect();
    let custom_path = match args.len()
    {
        0...1 => None,
        _ => Some(args[2].clone()),
    };
    let config = match try_open_config(custom_path)
    {
        Some(f) =>
        {
            match serde_json::from_reader(f)
            {
                Ok(x) => x,
                Err(why) => panic!("Invalid JSON file: {}", why),
            }
        }
        None => ServerConfig::new(),
    };
    let mut server = Server::new(config);
    info!("dazzled started successfully!");
    loop
    {
        server.main();
    }
}
