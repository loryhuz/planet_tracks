//! A track drawn by hand (the track editor's draft): a chain of blocks from a start to a
//! finish, grown by strokes of a brush and changed a stretch at a time.
//!
//! A stroke is a line drawn over the grid seen from above. It is never laid as it is: [`fit`]
//! finds the chain of catalogue blocks (straights and turns of one to three cells) whose
//! centreline follows it best, joined on the grid, so the draft is always a valid track. A
//! stroke that starts on the draft redraws it from there; one that starts elsewhere begins a
//! new draft, its start block where the stroke starts, heading the way it goes.
//!
//! A stretch of the draft (a range of its blocks) takes an [`Edit`]: its surface, its height
//! (raised or lowered a level, with a slope at each end), a hump, a row of whoops, a jump, its
//! turns banked or flat, a checkpoint, or the end of the draft cut off there. Heights cost
//! length: a slope takes two or three cells of straight (or a turn that can climb, see
//! [`kit::climb_fits`]), so an edit that has no room says so rather than making a wall.
//!
//! Where a road and a dirt track meet, [`Draft::settle`] puts a transition block (`to_dirt`,
//! `to_road`) on a straight cell at the join.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use glam::{Vec2, Vec3};

use crate::Surface;
use crate::kit::{CELL, Connector, Gate, Heading, Kind, LEVEL, Layout, Piece, Placed, Side, climb_fits};
use crate::map::{BANK_DEG, FORMAT, Map, MapError, TimeOfDay, blocks_from_layout};
use crate::terrain::TerrainSettings;

/// Spacing of the points a stroke and a block's centreline are compared at, metres.
const STEP: f32 = 4.0;
/// A block whose centreline strays farther than this from the stroke is not a candidate, metres.
const MAX_DEVIATION: f32 = 32.0;
/// Deviation that costs as much as a block, metres.
const SIGMA: f32 = 7.0;
/// Most stroke points a block's next sample may skip ahead to.
const REACH: usize = 7;
/// A fit is done when its last block ends this close to the end of the stroke, metres.
const END_SLACK: f32 = 14.0;
/// Most states a fit explores before it settles for the furthest it got.
const MAX_EXPANSIONS: usize = 60_000;
/// Highest level an edit raises a stretch to.
const MAX_LEVEL: i32 = 6;

/// What the brush lays: the deck of new blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deck {
    Road,
    Dirt,
    /// A road with booster arrows.
    Booster,
}

impl Deck {
    fn of(piece: &Piece) -> Deck {
        match (piece.deck, piece.boost) {
            (Surface::Dirt, _) => Deck::Dirt,
            (_, true) => Deck::Booster,
            _ => Deck::Road,
        }
    }

    /// `piece` with this deck (a gate or a transition takes a plain road for a booster).
    fn apply(self, mut piece: Piece) -> Piece {
        let plain = piece.gate.is_some() || matches!(piece.kind, Kind::Transition { .. });
        piece.deck = if self == Deck::Dirt { Surface::Dirt } else { Surface::Road };
        piece.boost = self == Deck::Booster && !plain;
        piece
    }
}

/// A change to a stretch of the draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Surface(Deck),
    /// One level up (+1) or down (-1), a slope at each end.
    Height(i32),
    /// A crest: up a level and straight back down, on the longest straight of the stretch.
    Hump,
    /// The straights of the stretch become whoops.
    Whoops,
    /// A slope up a level, a jump ramp, and a landing back down, on its longest straight.
    Jump,
    /// The turns of the stretch banked (berms on the ground, banked turns up on stilts), or flat.
    Bank(bool),
    /// Adds a checkpoint in the middle of the stretch, or removes those it has.
    Checkpoint,
    /// Cuts the draft off at the stretch's first block (the finish moves there).
    Cut,
}

/// Why an edit changed nothing, in words for the player.
pub type EditError = String;

/// The editor's draft: blocks chained from the entry of the start block to the finish block.
#[derive(Clone, Debug)]
pub struct Draft {
    pub name: String,
    /// Where the car enters the start block.
    pub entry: Connector,
    /// Start first and finish last; empty before the first stroke.
    pub pieces: Vec<Piece>,
}

impl Draft {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), entry: Connector::entering((0, 0), 0, Heading::North), pieces: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }

    /// The blocks placed on the grid.
    pub fn layout(&self) -> Layout {
        let mut layout = Layout::new(&self.name, self.entry);
        for p in &self.pieces {
            layout.push(*p);
        }
        layout
    }

    /// The draft as a map, raced on Mars terrain; `None` while it is empty. Its version is a hash
    /// of its blocks, so records (and the ghost) are kept per layout.
    pub fn to_map(&self) -> Option<Map> {
        if self.pieces.len() < 2 {
            return None;
        }
        let blocks = blocks_from_layout(&self.layout()).ok()?;
        let mut hash: u32 = 0x811c_9dc5;
        for b in &blocks {
            let text = format!("{}{:?}{}{}{:?}", b.block, b.cell, b.level, b.rotation, b.variant);
            for byte in text.bytes() {
                hash = (hash ^ byte as u32).wrapping_mul(0x0100_0193);
            }
        }
        Some(Map {
            format: FORMAT,
            name: self.name.clone(),
            author: String::new(),
            version: hash,
            planet: crate::Planet::Mars,
            terrain: TerrainSettings { seed: 2026, ..TerrainSettings::default() },
            time: TimeOfDay::Day,
            blocks,
            landforms: Vec::new(),
            structures: Vec::new(),
            scenery: Vec::new(),
        })
    }

    /// The route of a map, as a draft.
    pub fn from_map(map: &Map) -> Result<Draft, MapError> {
        let layout = map.layout()?;
        Ok(Draft { name: map.name.clone(), entry: layout.start, pieces: layout.pieces.iter().map(|p| p.piece).collect() })
    }

    /// Draws a stroke with the brush laying `deck`: redraws the draft from the block the stroke
    /// starts on, or begins a new draft where it starts. Returns whether the draft changed.
    pub fn draw(&mut self, stroke: &[Vec2], deck: Deck) -> bool {
        let points = resample(stroke);
        if points.len() < 3 {
            return false;
        }
        let placed = self.layout().pieces;
        let on = (0..placed.len()).rev().find(|&i| near_piece(&placed[i], points[0], 0.0));
        let (keep, from) = match on {
            // On the finish: carry on from its entry.
            Some(i) if i + 1 == placed.len() && i > 0 => (i, placed[i].entry),
            Some(i) => (i + 1, placed[i].exit),
            None => {
                let Some(heading) = initial_heading(&points) else { return false };
                let cell = cell_of(points[0]);
                let entry = Connector::entering(cell, 0, heading);
                let start = Placed::new(start_piece(deck), entry);
                let fitted = fit(start.exit, &points, deck);
                if fitted.is_empty() {
                    return false;
                }
                let before = (self.pieces.clone(), self.entry);
                self.entry = entry;
                self.pieces = std::iter::once(start_piece(deck)).chain(fitted).collect();
                self.close(deck);
                if self.check().is_err() {
                    (self.pieces, self.entry) = before;
                    return false;
                }
                return true;
            }
        };
        let fitted = fit(from, &points, deck);
        if fitted.is_empty() {
            return false;
        }
        let before = self.pieces.clone();
        self.pieces.truncate(keep);
        self.pieces.extend(fitted);
        self.close(deck);
        if self.check().is_err() {
            self.pieces = before;
            return false;
        }
        true
    }

    /// Puts a finish after the last block, then settles the joins.
    fn close(&mut self, deck: Deck) {
        let deck = if deck == Deck::Dirt { Deck::Dirt } else { Deck::Road };
        self.pieces.push(deck.apply(Piece::road(Kind::Straight { cells: 1 }).gate(Gate::Finish)));
        self.settle();
    }

    /// Transition blocks where a road and a dirt track meet: on the first straight cell of the
    /// join (the second block, or else the first), whose second half takes the next deck.
    pub fn settle(&mut self) {
        let n = self.pieces.len();
        // A transition between blocks of the same deck is a plain straight again; one still
        // needed takes its neighbours' decks.
        for i in 1..n.saturating_sub(1) {
            if !matches!(self.pieces[i].kind, Kind::Transition { .. }) {
                continue;
            }
            let (before, after) = (exit_deck(&self.pieces[i - 1]), self.pieces[i + 1].deck);
            self.pieces[i] = if before == after { Piece { deck: before, ..Piece::road(Kind::Straight { cells: 1 }) } } else { transition(before, after) };
        }
        for i in 0..n.saturating_sub(1) {
            let out = exit_deck(&self.pieces[i]);
            let next = self.pieces[i + 1];
            if out == next.deck || matches!(next.kind, Kind::Transition { .. }) {
                continue;
            }
            if plain_straight(&next) {
                self.pieces[i + 1] = transition(out, next.deck);
            } else if plain_straight(&self.pieces[i]) && !matches!(self.pieces[i].kind, Kind::Transition { .. }) {
                self.pieces[i] = transition(self.pieces[i].deck, next.deck);
            }
        }
    }

    /// Pairs of blocks on the same cell at nearly the same height (they cross or overlap).
    pub fn overlaps(&self) -> Vec<usize> {
        let placed = self.layout().pieces;
        let mut used: BTreeMap<(i32, i32), Vec<(usize, f32, f32)>> = BTreeMap::new();
        let mut out = Vec::new();
        for (i, p) in placed.iter().enumerate() {
            let (lo, hi) = (p.entry.pos.y.min(p.exit.pos.y), p.entry.pos.y.max(p.exit.pos.y));
            for c in p.cells() {
                let list = used.entry(c).or_default();
                for &(j, l2, h2) in list.iter() {
                    if j + 1 != i && j != i && lo < h2 + 6.0 && l2 < hi + 6.0 {
                        out.push(j);
                        out.push(i);
                    }
                }
                list.push((i, lo, hi));
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The blocks a stroke passes over: the first and last of them, in chain order.
    pub fn touched(&self, stroke: &[Vec2]) -> Option<(usize, usize)> {
        let placed = self.layout().pieces;
        let mut range: Option<(usize, usize)> = None;
        for &q in &resample(stroke) {
            for (i, p) in placed.iter().enumerate() {
                if near_piece(p, q, 0.0) {
                    range = Some(range.map_or((i, i), |(a, b)| (a.min(i), b.max(i))));
                }
            }
        }
        range
    }

    /// Applies `edit` to blocks `a..=b`.
    pub fn edit(&mut self, (a, b): (usize, usize), edit: Edit) -> Result<(), EditError> {
        if self.pieces.is_empty() || a > b || b >= self.pieces.len() {
            return Err("Rien de sélectionné".into());
        }
        let (before, entry) = (self.pieces.clone(), self.entry);
        let result = match edit {
            Edit::Surface(deck) => {
                for p in &mut self.pieces[a..=b] {
                    if let Kind::Transition { to } = p.kind {
                        *p = Piece { deck: to, ..Piece::road(Kind::Straight { cells: 1 }) };
                    }
                    *p = deck.apply(*p);
                }
                Ok(())
            }
            Edit::Height(dir) => self.raise(a, b, dir.signum()),
            Edit::Hump => self.hump(a, b),
            Edit::Whoops => self.whoops(a, b),
            Edit::Jump => self.jump(a, b),
            Edit::Bank(on) => self.bank(a, b, on),
            Edit::Checkpoint => self.checkpoint(a, b),
            Edit::Cut => {
                if a == 0 {
                    self.pieces.clear();
                    return Ok(());
                }
                let deck = Deck::of(&self.pieces[a - 1]);
                self.pieces.truncate(a);
                if self.pieces.len() == 1 {
                    self.pieces.clear();
                    return Ok(());
                }
                self.close(deck);
                Ok(())
            }
        };
        if result.is_ok() {
            self.settle();
            // No block may leave the grid's levels or go under the ground.
            let layout = self.layout();
            if layout.pieces.iter().any(|p| p.exit.pos.y < -0.01 || p.entry.pos.y < -0.01) {
                (self.pieces, self.entry) = (before, entry);
                return Err("Ça passerait sous le sol".into());
            }
            // Every block must be one of the catalogue's, the map one the game can load.
            if let Err(e) = self.check() {
                (self.pieces, self.entry) = (before, entry);
                return Err(format!("Impossible ici : {e}"));
            }
        } else {
            (self.pieces, self.entry) = (before, entry);
        }
        result
    }

    /// Whether the draft makes a map the game loads: its blocks are the catalogue's, its route
    /// joins them from the start to the finish (an empty draft passes).
    pub fn check(&self) -> Result<(), MapError> {
        let Some(map) = self.to_map() else {
            return if self.pieces.len() < 2 { Ok(()) } else { Err(MapError::NotInCatalogue("bloc inconnu".into())) };
        };
        Map::load(&map.to_json()).map(|_| ())
    }

    /// The level each block is entered at.
    fn levels(&self) -> Vec<i32> {
        self.layout().pieces.iter().map(|p| level_of(&p.entry)).collect()
    }

    /// A ramp giving the road `delta` levels more of climb (in the driving direction), found
    /// going through the blocks `order` (away from a stretch, or into it): the nearest slope or
    /// climbing turn takes it (a slope up a level becomes flat again, or two levels on three
    /// cells), or else the first run of plain straights (the 2 or 3 nearest become a slope) or
    /// turn that can climb. `None` when there is none. Returns the index range the ramp replaces
    /// and its blocks.
    fn ramp_along(&self, order: &[usize], delta: i32) -> Option<((usize, usize), Vec<Piece>)> {
        let forward = order.windows(2).next().is_none_or(|w| w[1] > w[0]);
        for (n, &i) in order.iter().enumerate() {
            let p = self.pieces[i];
            match p.kind {
                Kind::Slope { cells, levels } => {
                    let new = levels + delta;
                    if new == 0 {
                        return Some(((i, i), vec![Piece { kind: Kind::Straight { cells: 1 }, ..p }; cells as usize]));
                    }
                    if new.unsigned_abs() <= 2.min(cells) {
                        return Some(((i, i), vec![Piece { kind: Kind::Slope { cells, levels: new }, ..p }]));
                    }
                }
                Kind::Turn { levels, .. } | Kind::Curve { levels, .. } if levels != 0 => {
                    let flat = p.kind.climbing(0);
                    let new = levels + delta;
                    if let Some(kind) = if new == 0 { Some(flat) } else { climbing(flat, new) } {
                        return Some(((i, i), vec![Piece { kind, ..p }]));
                    }
                }
                _ => {}
            }
            if plain_straight(&p) {
                let run = order[n..].iter().take_while(|&&j| plain_straight(&self.pieces[j])).count().min(3);
                if run >= 2 {
                    let (lo, hi) = if forward { (i, i + run - 1) } else { (i + 1 - run, i) };
                    return Some(((lo, hi), vec![Piece { kind: Kind::Slope { cells: run as u32, levels: delta }, ..p }]));
                }
            }
            if let Some(kind) = climbing(p.kind, delta) {
                return Some(((i, i), vec![Piece { kind, ..p }]));
            }
        }
        None
    }

    /// The stretch a level up or down, a ramp before and after it: outside it (the blocks between
    /// the ramp and the stretch move with it), or else at its own ends. Selected, the start or
    /// the finish stays up there, with nothing beyond it: selecting the start raises it.
    fn raise(&mut self, a: usize, b: usize, dir: i32) -> Result<(), EditError> {
        let levels = self.levels();
        let (lo, hi) = (*levels[a..=b].iter().min().unwrap(), *levels[a..=b].iter().max().unwrap());
        if dir < 0 && lo == 0 {
            return Err("Déjà au sol : sélectionne une partie surélevée".into());
        }
        if dir > 0 && hi >= MAX_LEVEL {
            return Err("Déjà tout en haut".into());
        }
        let last = self.pieces.len() - 1;
        let (before, after): (Vec<usize>, Vec<usize>) = ((0..a).rev().collect(), (b + 1..=last).collect());
        let (inside, inside_back): (Vec<usize>, Vec<usize>) = ((a..=b).collect(), (a..=b).rev().collect());
        let up = self.ramp_along(&before, dir).or_else(|| if a > 0 { self.ramp_along(&inside, dir) } else { None });
        let down = self.ramp_along(&after, -dir).or_else(|| if b < last { self.ramp_along(&inside_back, -dir) } else { None });
        if let (Some(((_, up_end), _)), Some(((down_start, _), _))) = (&up, &down) {
            if down_start <= up_end {
                return Err("Trop court pour monter et redescendre : sélectionne un peu plus large".into());
            }
        }
        if let Some(((lo, hi), pieces)) = down {
            self.pieces.splice(lo..=hi, pieces);
        }
        match up {
            Some(((lo, hi), pieces)) => {
                self.pieces.splice(lo..=hi, pieces);
            }
            // Nothing before it: the start goes up (or down) with the stretch.
            None => self.entry.pos.y += dir as f32 * LEVEL,
        }
        Ok(())
    }

    /// Takes one level out of the first descent from block `from` on (a slope down, or a turn
    /// that descends): after a jump that lands a level lower, the road is already down there.
    fn ease_next_descent(&mut self, from: usize) {
        for i in from..self.pieces.len() {
            let p = self.pieces[i];
            match p.kind {
                Kind::Slope { cells, levels: -1 } => {
                    let flat = Piece { kind: Kind::Straight { cells: 1 }, ..p };
                    self.pieces.splice(i..=i, std::iter::repeat_n(flat, cells as usize));
                    return;
                }
                Kind::Slope { cells, levels } if levels < -1 => {
                    self.pieces[i].kind = Kind::Slope { cells, levels: levels + 1 };
                    return;
                }
                Kind::Turn { levels, .. } | Kind::Curve { levels, .. } if levels < 0 => {
                    let flat = p.kind.climbing(0);
                    let eased = if levels == -1 { Some(flat) } else { climbing(flat, levels + 1) };
                    if let Some(kind) = eased {
                        self.pieces[i].kind = kind;
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// What a stretch is made of, in words: its blocks and its height.
    pub fn describe(&self, (a, b): (usize, usize)) -> String {
        if a > b || b >= self.pieces.len() {
            return String::new();
        }
        let placed = self.layout().pieces;
        let (mut straights, mut turns, mut others) = (0, 0, 0);
        for p in &self.pieces[a..=b] {
            match p.kind {
                Kind::Straight { cells } if p.gate.is_none() => straights += cells,
                Kind::Turn { .. } | Kind::Curve { .. } => turns += 1,
                _ => others += 1,
            }
        }
        let plural = |n: u32, word: &str| format!("{n} {word}{}", if n > 1 { "s" } else { "" });
        let mut parts = vec![plural(straights, "droite")];
        if turns > 0 {
            parts.push(plural(turns, "virage"));
        }
        if others > 0 {
            parts.push(plural(others, "autre"));
        }
        let heights: Vec<i32> = placed[a..=b].iter().flat_map(|p| [level_of(&p.entry), level_of(&p.exit)]).collect();
        let metres = |l: i32| l * LEVEL as i32;
        let height = match (metres(*heights.iter().min().unwrap()), metres(*heights.iter().max().unwrap())) {
            (0, 0) => "au sol".to_string(),
            (l, h) if l == h => format!("à {l} m"),
            (l, h) => format!("de {l} à {h} m"),
        };
        format!("{} : {} · {height}", plural((b - a + 1) as u32, "bloc"), parts.join(", "))
    }

    /// The longest run of plain straights in `a..=b`.
    fn longest_run(&self, a: usize, b: usize) -> Option<(usize, usize)> {
        let mut best: Option<(usize, usize)> = None;
        let mut i = a;
        while i <= b {
            if plain_straight(&self.pieces[i]) {
                let mut j = i;
                while j < b && plain_straight(&self.pieces[j + 1]) {
                    j += 1;
                }
                if best.is_none_or(|(s, e)| j - i > e - s) {
                    best = Some((i, j));
                }
                i = j + 1;
            } else {
                i += 1;
            }
        }
        best
    }

    fn hump(&mut self, a: usize, b: usize) -> Result<(), EditError> {
        let run = self.longest_run(a, b).map_or(0, |(s, e)| e - s + 1);
        let (s, e) = self.longest_run(a, b).filter(|(s, e)| e > s).ok_or(format!("Bosse : il faut une ligne droite de 2 cases, la plus longue ici fait {run}"))?;
        let n = e - s + 1;
        let half = (n / 2).min(3);
        let first = s + (n - 2 * half) / 2;
        let p = self.pieces[first];
        let up = Piece { kind: Kind::Slope { cells: half as u32, levels: 1 }, ..p };
        let down = Piece { kind: Kind::Slope { cells: half as u32, levels: -1 }, ..p };
        self.pieces.splice(first..first + 2 * half, [up, down]);
        Ok(())
    }

    fn whoops(&mut self, a: usize, b: usize) -> Result<(), EditError> {
        let mut runs = Vec::new();
        let mut i = a;
        while i <= b {
            if plain_straight(&self.pieces[i]) {
                let mut j = i;
                while j < b && plain_straight(&self.pieces[j + 1]) {
                    j += 1;
                }
                runs.push((i, j));
                i = j + 1;
            } else {
                i += 1;
            }
        }
        if runs.is_empty() {
            return Err("Petites bosses : il faut une case droite dans la sélection".into());
        }
        for &(s, e) in runs.iter().rev() {
            let n = (e - s + 1) as u32;
            let p = self.pieces[s];
            self.pieces.splice(s..=e, [Piece { kind: Kind::Whoops { cells: n, bumps: n, height: crate::map::WHOOPS_HEIGHT }, ..p }]);
        }
        Ok(())
    }

    /// A jump on the longest straight of the stretch: a slope up a level, the ramp and a landing
    /// back down (7 cells); on a raised road the ramp and its landing alone (5 cells), the landing
    /// then doing the work of the next descent.
    fn jump(&mut self, a: usize, b: usize) -> Result<(), EditError> {
        let levels = self.levels();
        let (s, e) = self.longest_run(a, b).unwrap_or((a, a));
        let n = if plain_straight(&self.pieces[s]) { e - s + 1 } else { 0 };
        let p = self.pieces[s];
        let ramp = Piece { kind: Kind::JumpRamp { lip_deg: crate::map::LIP_DEG }, ..p };
        let landing = |cells: usize| -> Result<Piece, EditError> {
            let lp = crate::map::parse_block(&format!("landing{cells}_down1"), None).map_err(|_| "Saut impossible ici".to_string())?;
            Ok(Piece { kind: lp.kind, ..p })
        };
        if n >= 7 {
            let slope = if n >= 8 { 3 } else { 2 };
            let cells = (n - slope - 1).min(8);
            let first = s + (n - slope - 1 - cells) / 2;
            let up = Piece { kind: Kind::Slope { cells: slope as u32, levels: 1 }, ..p };
            self.pieces.splice(first..first + slope + 1 + cells, [up, ramp, landing(cells)?]);
            Ok(())
        } else if n >= 5 && levels[s] >= 1 {
            let cells = (n - 1).min(8);
            self.pieces.splice(s..s + 1 + cells, [ramp, landing(cells)?]);
            self.ease_next_descent(s + 2);
            Ok(())
        } else {
            Err(format!("Saut : il faut une ligne droite de 7 cases (5 sur une route surélevée), la plus longue ici fait {n}"))
        }
    }

    fn bank(&mut self, a: usize, b: usize, on: bool) -> Result<(), EditError> {
        let placed = self.layout().pieces;
        let mut changed = false;
        for i in a..=b {
            let Kind::Turn { size, side, quarters, levels, .. } = self.pieces[i].kind else { continue };
            let raised = level_of(&placed[i].entry) > 0 && level_of(&placed[i].exit) > 0;
            let (bank_deg, pivot) = match (on, raised) {
                (false, _) => (0.0, crate::kit::Pivot::Centre),
                (true, true) => (BANK_DEG, crate::kit::Pivot::Centre),
                (true, false) => (BANK_DEG, crate::kit::Pivot::Inner),
            };
            let kind = Kind::Turn { size, side, quarters, bank_deg, pivot, levels };
            if kind != self.pieces[i].kind && (levels == 0 || climb_fits(kind)) {
                self.pieces[i].kind = kind;
                changed = true;
            }
        }
        if changed { Ok(()) } else { Err("Pas de virage à changer dans la sélection".into()) }
    }

    fn checkpoint(&mut self, a: usize, b: usize) -> Result<(), EditError> {
        let gates: Vec<usize> = (a..=b).filter(|&i| self.pieces[i].gate == Some(Gate::Checkpoint)).collect();
        if !gates.is_empty() {
            for i in gates {
                self.pieces[i].gate = None;
            }
            return Ok(());
        }
        let mid = (a + b) / 2;
        let last = self.pieces.len() - 1;
        let best = (a..=b).filter(|&i| i > 0 && i < last && plain_straight(&self.pieces[i])).min_by_key(|&i| i.abs_diff(mid));
        let i = best.ok_or_else(|| "Checkpoint : il faut une case droite dans la sélection (pas un virage)".to_string())?;
        let p = &mut self.pieces[i];
        p.gate = Some(Gate::Checkpoint);
        p.boost = false;
        Ok(())
    }
}

/// `kind` climbing (or descending) `levels` as it turns, when it is a quarter turn that can (the
/// catalogue's climbing turns go up or down one or two levels).
fn climbing(kind: Kind, levels: i32) -> Option<Kind> {
    match kind {
        Kind::Turn { quarters: 1, levels: 0, .. } | Kind::Curve { levels: 0, .. } if levels != 0 && levels.abs() <= 2 => {
            let k = kind.climbing(levels);
            climb_fits(k).then_some(k)
        }
        _ => None,
    }
}

/// A straight cell without a gate: what slopes, whoops and transitions are made of.
fn plain_straight(p: &Piece) -> bool {
    p.kind == (Kind::Straight { cells: 1 }) && p.gate.is_none()
}

fn transition(from: Surface, to: Surface) -> Piece {
    let from = if from == Surface::Dirt { Surface::Dirt } else { Surface::Road };
    Piece { kind: Kind::Transition { to }, deck: from, ..Piece::road(Kind::Straight { cells: 1 }) }
}

/// The deck at a block's exit (a transition's second half).
fn exit_deck(p: &Piece) -> Surface {
    match p.kind {
        Kind::Transition { to } => to,
        _ if p.deck == Surface::Dirt => Surface::Dirt,
        _ => Surface::Road,
    }
}

fn start_piece(deck: Deck) -> Piece {
    let deck = if deck == Deck::Dirt { Deck::Dirt } else { Deck::Road };
    deck.apply(Piece::road(Kind::Straight { cells: 1 }).gate(Gate::Start))
}

fn level_of(c: &Connector) -> i32 {
    libm::floorf(c.pos.y / LEVEL + 0.5) as i32
}

fn cell_of(p: Vec2) -> (i32, i32) {
    (libm::floorf(p.x / CELL) as i32, libm::floorf(p.y / CELL) as i32)
}

/// Whether `q` (x, z) is on block `p`'s deck, or within `margin` of it.
fn near_piece(p: &Placed, q: Vec2, margin: f32) -> bool {
    let reach = p.piece.half_width(p.piece.deck).max(10.0) + 4.0 + margin;
    let n = libm::ceilf(p.length / STEP).max(1.0) as usize;
    (0..=n).any(|k| {
        let f = p.frame(p.length * k as f32 / n as f32);
        Vec2::new(f.horiz.x, f.horiz.z).distance(q) <= reach
    })
}

/// The way a stroke sets off, on the grid: along its first 24 m.
fn initial_heading(points: &[Vec2]) -> Option<Heading> {
    let d = points.iter().find(|q| q.distance(points[0]) >= 24.0).or(points.last())? - points[0];
    if d.length() < 8.0 {
        return None;
    }
    Some(if d.x.abs() > d.y.abs() {
        if d.x > 0.0 { Heading::West } else { Heading::East }
    } else if d.y > 0.0 {
        Heading::North
    } else {
        Heading::South
    })
}

/// A stroke as points [`STEP`] apart along it, lightly smoothed.
fn resample(stroke: &[Vec2]) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    let Some(&first) = stroke.first() else { return out };
    out.push(first);
    let mut carry = 0.0;
    for w in stroke.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        if len < 1e-4 {
            continue;
        }
        let mut t = STEP - carry;
        while t <= len {
            out.push(a + (b - a) * (t / len));
            t += STEP;
        }
        carry = len - (t - STEP);
    }
    if let Some(&last) = stroke.last() {
        if out.last().is_some_and(|l| l.distance(last) > 0.5 * STEP) {
            out.push(last);
        }
    }
    // A finger shakes: average each point with its neighbours (the ends stay).
    let mut smooth = out.clone();
    for i in 1..out.len().saturating_sub(1) {
        smooth[i] = (out[i - 1] + 2.0 * out[i] + out[i + 1]) * 0.25;
    }
    smooth
}

/// A candidate block for the fit, with its centreline seen from an entry at the origin heading
/// north: (left, forward) offsets of points [`STEP`] apart.
struct Shape {
    piece: Piece,
    samples: Vec<Vec2>,
    /// Exit offset and quarter turns.
    exit: Vec2,
    quarters: i32,
    cost: f32,
    /// A hairpin's side (1 left, -1 right), 0 for any other block.
    hairpin: i32,
}

/// Cost of a block turning more or less than the stroke does over the same stretch, per
/// radian squared: a gentle curve is not read as a hairpin, a sharp corner is.
const TURN_MISMATCH: f32 = 4.0;

/// Extra cost of a hairpin right after one to the other side: a zigzag the grid would make of a
/// diagonal, where an S-bend or a wider turn reads the stroke better.
const ZIGZAG: f32 = 3.0;

fn shapes(deck: Deck) -> Vec<Shape> {
    let mut kinds = vec![(Kind::Straight { cells: 1 }, 0.3)];
    for size in 1..=3u32 {
        // A one-cell turn is a hairpin: taken only where the stroke turns sharply.
        let cost = if size == 1 { 1.6 } else { 0.7 };
        for side in [Side::Left, Side::Right] {
            let kind = if deck == Deck::Dirt { Kind::berm(size, side, 1, BANK_DEG) } else { Kind::turn(size, side) };
            kinds.push((kind, cost));
        }
    }
    // S-bends one cell sideways: the grid's diagonals (a stroke at an angle would otherwise be
    // a staircase of hairpins).
    for cells in 2..=4u32 {
        for shift in [1, -1] {
            kinds.push((Kind::Shift { cells, shift }, 0.5));
        }
    }
    let origin = Connector::entering((0, 0), 0, Heading::North);
    kinds
        .into_iter()
        .map(|(kind, cost)| {
            let piece = deck.apply(Piece::road(kind));
            let placed = Placed::new(piece, origin);
            let n = libm::ceilf(placed.length / STEP).max(2.0) as usize;
            let local = |p: Vec3| Vec2::new(p.x - origin.pos.x, p.z - origin.pos.z);
            let samples = (1..=n).map(|k| local(placed.frame(placed.length * k as f32 / n as f32).horiz)).collect();
            let quarters = match placed.exit.heading {
                Heading::North => 0,
                Heading::West => 1,
                Heading::South => 2,
                Heading::East => -1,
            };
            let hairpin = match kind {
                Kind::Turn { size: 1, side, .. } => side.sign() as i32,
                _ => 0,
            };
            Shape { piece, samples, exit: local(placed.exit.pos), quarters, cost, hairpin }
        })
        .collect()
}

/// An angle in (-π, π].
fn wrap(a: f32) -> f32 {
    let tau = 2.0 * core::f32::consts::PI;
    let a = a - tau * libm::floorf(a / tau);
    if a > core::f32::consts::PI { a - tau } else { a }
}

/// `offset` (left, forward) from a connector heading `h`, in world (x, z).
fn rotate(h: Heading, offset: Vec2) -> Vec2 {
    let f = h.forward();
    let l = h.left();
    Vec2::new(l.x * offset.x + f.x * offset.y, l.z * offset.x + f.z * offset.y)
}

#[derive(Clone, Copy)]
struct Node {
    parent: Option<usize>,
    shape: usize,
    at: Connector,
    /// Index of the stroke point the block's end is matched with.
    k: usize,
    cost: f32,
    hairpin: i32,
}

struct Open(f32, usize);

impl PartialEq for Open {
    fn eq(&self, o: &Self) -> bool {
        self.0 == o.0 && self.1 == o.1
    }
}
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Open {
    // The cheapest first, then the earliest made (deterministic).
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}

/// The chain of blocks from `from` that follows the stroke (points [`STEP`] apart) best: each
/// block's centreline is matched point by point with the stroke, moving forward along it, and
/// costs its deviation from it plus a price per block (a hairpin costs more). The cheapest chain
/// that reaches the end of the stroke wins; a stroke no chain can follow to its end gets the one
/// that follows it furthest.
pub fn fit(from: Connector, points: &[Vec2], deck: Deck) -> Vec<Piece> {
    let shapes = shapes(deck);
    let m = points.len() - 1;
    let start = Vec2::new(from.pos.x, from.pos.z);
    let k0 = (0..=m.min(40)).min_by(|&a, &b| points[a].distance(start).total_cmp(&points[b].distance(start))).unwrap_or(0);
    let end_k = m.saturating_sub((END_SLACK / STEP) as usize);
    if k0 >= end_k {
        return Vec::new();
    }
    // The stroke's heading at each point, as a yaw (0 along +Z, positive to the left).
    let yaws: Vec<f32> = (0..=m)
        .map(|k| {
            let d = points[(k + 2).min(m)] - points[k.saturating_sub(2)];
            libm::atan2f(d.x, d.y)
        })
        .collect();
    let mut nodes = vec![Node { parent: None, shape: 0, at: from, k: k0, cost: 0.0, hairpin: 0 }];
    let mut open = BinaryHeap::from([Open(0.0, 0)]);
    let mut seen: BTreeMap<(i32, i32, u8, usize, i32), f32> = BTreeMap::new();
    let mut furthest = 0;
    let mut goal = None;
    let mut expansions = 0;
    while let Some(Open(cost, id)) = open.pop() {
        let node = nodes[id];
        if cost > node.cost + 1e-6 {
            continue;
        }
        if node.k >= end_k {
            goal = Some(id);
            break;
        }
        expansions += 1;
        if expansions > MAX_EXPANSIONS {
            break;
        }
        let origin = Vec2::new(node.at.pos.x, node.at.pos.z);
        'shapes: for (s, shape) in shapes.iter().enumerate() {
            let mut k = node.k;
            let mut dev = 0.0;
            for &local in &shape.samples {
                let p = origin + rotate(node.at.heading, local);
                let mut best = (points[k].distance_squared(p), k);
                for kk in k + 1..=(k + REACH).min(m) {
                    let d = points[kk].distance_squared(p);
                    if d < best.0 {
                        best = (d, kk);
                    }
                }
                let d = libm::sqrtf(best.0);
                if d > MAX_DEVIATION {
                    continue 'shapes;
                }
                // Stroke skipped over (a detour the blocks cut across) costs too.
                let skipped = (best.1 - k).saturating_sub(2) as f32;
                // Squared near the stroke, linear farther (a wide turn across a gentle curve).
                let e = d / SIGMA;
                dev += if e < 1.0 { e * e } else { 2.0 * e - 1.0 } + 0.5 * skipped;
                k = best.1;
            }
            if k <= node.k {
                continue;
            }
            let zigzag = if shape.hairpin != 0 && shape.hairpin == -node.hairpin { ZIGZAG } else { 0.0 };
            let turned = wrap(yaws[k] - yaws[node.k]);
            let mismatch = wrap(shape.quarters as f32 * core::f32::consts::FRAC_PI_2 - turned);
            let step = dev * (STEP / CELL) + shape.cost + zigzag + TURN_MISMATCH * mismatch * mismatch;
            let exit = origin + rotate(node.at.heading, shape.exit);
            let heading = node.at.heading.turned(shape.quarters);
            let at = Connector { pos: Vec3::new(exit.x, node.at.pos.y, exit.y), heading, grade: 0.0 };
            let cost = node.cost + step;
            let key = (libm::roundf(exit.x * 2.0 / CELL) as i32, libm::roundf(exit.y * 2.0 / CELL) as i32, heading.rotation(), k / 2, shape.hairpin);
            if seen.get(&key).is_some_and(|&c| c <= cost) {
                continue;
            }
            seen.insert(key, cost);
            nodes.push(Node { parent: Some(id), shape: s, at, k, cost, hairpin: shape.hairpin });
            let new = nodes.len() - 1;
            if nodes[new].k > nodes[furthest].k || (nodes[new].k == nodes[furthest].k && cost < nodes[furthest].cost) {
                furthest = new;
            }
            open.push(Open(cost, new));
        }
    }
    let mut id = goal.unwrap_or(furthest);
    let mut chain = Vec::new();
    while let Some(parent) = nodes[id].parent {
        chain.push(shapes[nodes[id].shape].piece);
        id = parent;
    }
    chain.reverse();
    chain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::block_id;

    fn names(d: &Draft) -> Vec<String> {
        d.pieces
            .iter()
            .map(|p| {
                let (id, v) = block_id(p).expect("catalogue block");
                v.map_or(id.clone(), |v| format!("{id}:{v}"))
            })
            .collect()
    }

    /// A polyline through cell centres (cell coordinates).
    fn cells(path: &[(f32, f32)]) -> Vec<Vec2> {
        path.iter().map(|&(x, z)| Vec2::new((x + 0.5) * CELL, (z + 0.5) * CELL)).collect()
    }

    /// An arc of `radius` m about `centre`, from angle `a0` to `a1` (radians, x = cos, z = sin).
    fn arc(centre: Vec2, radius: f32, a0: f32, a1: f32) -> Vec<Vec2> {
        (0..=40).map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / 40.0;
            centre + Vec2::new(libm::cosf(a), libm::sinf(a)) * radius
        })
        .collect()
    }

    fn builds(d: &Draft) {
        let map = d.to_map().expect("map");
        let json = map.to_json();
        let map = Map::load(&json).unwrap_or_else(|e| panic!("{e}: {:?}", names(d)));
        map.build_detailed().expect("build");
        assert!(d.overlaps().is_empty(), "overlaps in {:?}", names(d));
    }

    #[test]
    fn a_straight_stroke_lays_straights() {
        let mut d = Draft::new("t");
        assert!(d.draw(&cells(&[(0.0, 0.0), (0.0, 6.0)]), Deck::Road));
        let ids = names(&d);
        assert_eq!(ids.first().unwrap(), "start");
        assert_eq!(ids.last().unwrap(), "finish");
        assert!(ids[1..ids.len() - 1].iter().all(|i| i == "straight"), "{ids:?}");
        assert!(ids.len() >= 6, "{ids:?}");
        builds(&d);
    }

    #[test]
    fn a_wide_arc_is_a_wide_turn_and_a_corner_a_hairpin() {
        // Up north from cell (0, 0), then a quarter of a circle of 48 m turning right (to -X).
        let mut stroke = cells(&[(0.0, 0.0), (0.0, 2.0)]);
        let c = Vec2::new(0.5 * CELL - 48.0, 3.0 * CELL);
        stroke.extend(arc(c, 48.0, 0.0, std::f32::consts::FRAC_PI_2));
        stroke.push(stroke.last().unwrap() + Vec2::new(-3.0 * CELL, 0.0));
        let mut d = Draft::new("t");
        assert!(d.draw(&stroke, Deck::Road));
        let ids = names(&d);
        assert!(ids.contains(&"turn2_right".to_string()), "{ids:?}");
        builds(&d);

        let mut d = Draft::new("t");
        assert!(d.draw(&cells(&[(0.0, 0.0), (0.0, 4.0), (-4.0, 4.0)]), Deck::Road));
        let ids = names(&d);
        assert!(ids.contains(&"turn1_right".to_string()), "{ids:?}");
        builds(&d);

        // A sweeper of 80 m.
        let mut stroke = cells(&[(0.0, 0.0), (0.0, 2.0)]);
        let c = Vec2::new(0.5 * CELL + 80.0, 3.0 * CELL);
        stroke.extend(arc(c, 80.0, std::f32::consts::PI, std::f32::consts::FRAC_PI_2));
        let mut d = Draft::new("t");
        assert!(d.draw(&stroke, Deck::Road));
        let ids = names(&d);
        assert!(ids.contains(&"turn3_left".to_string()), "{ids:?}");
    }

    #[test]
    fn a_stroke_from_the_draft_redraws_it_from_there() {
        let mut d = Draft::new("t");
        d.draw(&cells(&[(0.0, 0.0), (0.0, 6.0)]), Deck::Road);
        let before = d.pieces.len();
        // From the finish on: carries on.
        assert!(d.draw(&cells(&[(0.0, 6.0), (0.0, 10.0)]), Deck::Dirt));
        assert!(d.pieces.len() > before);
        let ids = names(&d);
        assert!(ids.contains(&"to_dirt".to_string()), "{ids:?}");
        assert_eq!(ids.last().unwrap(), "finish:dirt");
        builds(&d);
        // From the third block: turns off there.
        assert!(d.draw(&cells(&[(0.0, 2.0), (0.0, 3.0), (4.0, 3.0)]), Deck::Road));
        let ids = names(&d);
        assert!(ids.iter().any(|i| i.starts_with("turn")), "{ids:?}");
        builds(&d);
    }

    #[test]
    fn a_long_loop_fits_quickly() {
        let mut stroke = Vec::new();
        let c = Vec2::new(0.0, 0.0);
        stroke.extend(arc(c, 300.0, 0.0, 1.8 * std::f32::consts::PI));
        let t = std::time::Instant::now();
        let mut d = Draft::new("t");
        assert!(d.draw(&stroke, Deck::Road));
        let ms = t.elapsed().as_secs_f32() * 1000.0;
        println!("loop: {} blocks in {ms:.1} ms: {:?}", d.pieces.len(), names(&d));
        assert!(d.pieces.len() > 12);
        // A wide curve is no zigzag of hairpins.
        let hairpins = names(&d).iter().filter(|i| i.starts_with("turn1")).count();
        assert!(hairpins <= 2, "{hairpins} hairpins");
        builds(&d);
    }

    fn straight_draft(n: usize) -> Draft {
        let mut d = Draft::new("t");
        d.draw(&cells(&[(0.0, 0.0), (0.0, n as f32)]), Deck::Road);
        d
    }

    #[test]
    fn a_raised_stretch_climbs_and_comes_down() {
        // Two cells in the middle go up a level; the slopes go around them.
        let mut d = straight_draft(12);
        d.edit((5, 6), Edit::Height(1)).unwrap();
        let ids = names(&d);
        assert!(ids.contains(&"slope3_up1".to_string()) && ids.contains(&"slope3_down1".to_string()), "{ids:?}");
        let up = ids.iter().position(|i| i == "slope3_up1").unwrap();
        assert_eq!(d.describe((up + 1, up + 2)), "2 blocs : 2 droites · à 8 m");
        builds(&d);
        // Up again, then back down.
        d.edit((up + 1, up + 2), Edit::Height(1)).unwrap();
        builds(&d);
        let top = d.levels().iter().position(|&l| l == 2).unwrap();
        d.edit((top, top), Edit::Height(-1)).unwrap();
        builds(&d);
        assert!(d.edit((0, 0), Edit::Height(-1)).unwrap_err().starts_with("Déjà au sol"));

        // The start selected: it goes up, the slope down after it.
        let mut d = straight_draft(8);
        d.edit((0, 1), Edit::Height(1)).unwrap();
        assert_eq!(level_of(&d.entry), 1);
        assert!(names(&d).contains(&"slope3_down1".to_string()), "{:?}", names(&d));
        builds(&d);

        // The start and the cell after it, a wide turn next (the player's case): the start is
        // raised, the turn comes down.
        let mut stroke = cells(&[(0.0, 0.0), (0.0, 1.0)]);
        stroke.extend(arc(Vec2::new(0.5 * CELL + 80.0, 2.0 * CELL), 80.0, std::f32::consts::PI, std::f32::consts::FRAC_PI_2));
        stroke.push(Vec2::new(0.5 * CELL + 6.0 * CELL, 2.0 * CELL + 80.0));
        let mut d = Draft::new("t");
        assert!(d.draw(&stroke, Deck::Road));
        d.edit((0, 1), Edit::Height(1)).unwrap();
        assert_eq!(level_of(&d.entry), 1, "{:?}", names(&d));
        builds(&d);
        // Up again and again: the turn after it descends two levels at most (the catalogue's),
        // the rest of the descent goes farther on; the draft always stays a valid map.
        for _ in 0..4 {
            let _ = d.edit((0, 1), Edit::Height(1));
            d.check().unwrap_or_else(|e| panic!("{e}: {:?}", names(&d)));
        }
        assert!(level_of(&d.entry) >= 2, "{:?}", names(&d));
        builds(&d);

        // A raised stretch between two turns: the turns climb.
        let mut d = Draft::new("t");
        d.draw(&cells(&[(0.0, 0.0), (0.0, 4.0), (-4.0, 4.0), (-4.0, 8.0)]), Deck::Road);
        let turns: Vec<usize> = (0..d.pieces.len()).filter(|&i| matches!(d.pieces[i].kind, Kind::Turn { .. })).collect();
        if let [t0, t1] = turns[..] {
            d.edit((t0 + 1, t1 - 1), Edit::Height(1)).unwrap();
            builds(&d);
        }
    }

    #[test]
    fn humps_whoops_and_jumps_build() {
        let mut d = straight_draft(14);
        let n = d.pieces.len();
        d.edit((1, n - 2), Edit::Jump).unwrap();
        let ids_jump = names(&d);
        assert!(ids_jump.contains(&"jump_ramp".to_string()), "{ids_jump:?}");
        builds(&d);

        // On a raised stretch of 5 cells: the ramp and the landing alone, which takes the place
        // of the slope down.
        let mut d = straight_draft(16);
        d.edit((5, 11), Edit::Height(1)).unwrap();
        let first = d.levels().iter().position(|&l| l == 1).unwrap() + 1;
        d.edit((first, first + 4), Edit::Jump).unwrap();
        let ids = names(&d);
        assert!(ids.contains(&"jump_ramp".to_string()) && !ids.contains(&"slope3_down1".to_string()), "{ids:?}");
        builds(&d);
        let err = straight_draft(4).edit((1, 2), Edit::Jump).unwrap_err();
        assert!(err.ends_with("la plus longue ici fait 2"), "{err}");

        let mut d = straight_draft(8);
        d.edit((2, 5), Edit::Hump).unwrap();
        assert!(names(&d).contains(&"slope2_up1".to_string()), "{:?}", names(&d));
        builds(&d);

        let mut d = straight_draft(8);
        d.edit((2, 5), Edit::Whoops).unwrap();
        assert!(names(&d).contains(&"whoops4".to_string()), "{:?}", names(&d));
        builds(&d);
    }

    #[test]
    fn surfaces_banks_and_checkpoints() {
        let mut d = Draft::new("t");
        d.draw(&cells(&[(0.0, 0.0), (0.0, 6.0), (-6.0, 6.0)]), Deck::Road);
        let n = d.pieces.len();
        d.edit((3, n - 3), Edit::Surface(Deck::Dirt)).unwrap();
        let ids1 = names(&d);
        assert!(ids1.contains(&"to_dirt".to_string()) && ids1.contains(&"to_road".to_string()), "{ids1:?}");
        builds(&d);
        d.edit((0, n - 1), Edit::Bank(true)).unwrap();
        assert!(names(&d).iter().any(|i| i.starts_with("berm")), "{:?}", names(&d));
        builds(&d);
        d.edit((1, n - 2), Edit::Checkpoint).unwrap();
        assert!(names(&d).iter().any(|i| i.starts_with("checkpoint")), "{:?}", names(&d));
        builds(&d);
        d.edit((1, n - 2), Edit::Surface(Deck::Booster)).unwrap();
        builds(&d);
        d.edit((4, 4), Edit::Cut).unwrap();
        assert_eq!(d.pieces.len(), 5);
        builds(&d);
    }
}
