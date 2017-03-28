/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 *
 * Copyright 2017 - Dario Ostuni <dario.ostuni@gmail.com>
 *
 */

use std::default::Default;
use std::hash::Hasher;
use twox_hash::XxHash;

pub struct CHasher
{
    internal: XxHash,
}

impl Default for CHasher
{
    fn default() -> CHasher
    {
        CHasher::new()
    }
}

impl CHasher
{
    pub fn new() -> CHasher
    {
        CHasher { internal: XxHash::with_seed(0) }
    }

    pub fn update(&mut self, data: &[u8])
    {
        self.internal.write(data);
    }

    pub fn finalize(self) -> u64
    {
        self.internal.finish()
    }
}
