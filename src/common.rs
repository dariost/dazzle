/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 *
 * Copyright 2017 - Dario Ostuni <dario.ostuni@gmail.com>
 *
 */

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct Point
{
    pub x: usize,
    pub y: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Player
{
    pub name: String,
    pub points: u64,
    pub position: Point,
    pub id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Overview
{
    pub players: Vec<Player>,
    pub grid: Vec<Vec<Option<u64>>>,
    pub turns_left: u64,
    pub ms_for_turn: u64,
    pub tokens: Vec<Point>,
    pub game_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlayerInfo
{
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientRole
{
    Viewer,
    Player(PlayerInfo),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Direction
{
    Up,
    Down,
    Left,
    Right,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientCommand
{
    Move(Direction),
    Nothing,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerResponse
{
    Ok,
    Error(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ClientMessage
{
    HandShake(ClientRole),
    Command(ClientCommand),
}
