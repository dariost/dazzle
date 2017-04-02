/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 *
 * Copyright 2017 - Dario Ostuni <dario.ostuni@gmail.com>
 *
 */

use common::*;
use rand::{Rng, StdRng};
use rand::distributions::{IndependentSample, Range};
use std::collections::{HashMap, HashSet, VecDeque};

pub struct Game
{
    pub grid: Vec<Vec<Option<u64>>>,
    pub tokens: HashSet<Point>,
    pub turns_left: u64,
    pub rng: StdRng,
    pub gen_col: Range<usize>,
    pub gen_row: Range<usize>,
    pub game_id: u64,
    pub players: HashMap<u64, Player>,
    pub moved: HashMap<u64, bool>,
}

impl Game
{
    pub fn new(players: HashMap<u64, Player>, total_turns: u64, game_id: u64) -> Game
    {
        let mut players = players;
        let num_players = players.len();
        let rows = 8 + num_players / 2;
        let cols = rows * 2 + 1;
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
        let mut cap = 0.025 * (self.players.len() as f64).log2();
        if cap > 0.5
        {
            cap = 0.5;
        }
        while self.rng.next_f64() < cap
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
                    Direction::Down if player.position.y + 1 < rows &&
                                     !positions.contains(&Point {
                                                              x: player.position.x,
                                                              y: player.position.y + 1,
                                                          }) =>
                    {
                        player.position.y += 1;
                        Ok(())
                    }
                    Direction::Up if (player.position.y as i64) - 1 >= 0 &&
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
