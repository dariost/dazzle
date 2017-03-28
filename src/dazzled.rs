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

mod chasher;
mod common;
mod server;

use server::Server;

fn main()
{
    mowl::init_with_level(log::LogLevel::Info).unwrap();
    info!("Starting dazzled...");
    let mut server = Server::new();
    loop
    {
        server.main();
    }
}
