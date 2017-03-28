/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 *
 * Copyright 2017 - Dario Ostuni <dario.ostuni@gmail.com>
 *
 */

#[derive(Serialize, Deserialize, Debug)]
struct Point
{
    x: u32,
    y: u32,
}

#[derive(Serialize, Deserialize, Debug)]
struct Player
{
    name: String,
    points: u64,
    position: Point,
    id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct Overview
{
    players: Vec<Player>,
    grid: Vec<Vec<Option<u64>>>,
    turns_left: u64,
    ms_for_turn: u64,
    tokens: Vec<Point>,
}
