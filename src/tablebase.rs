use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;

use cozy_chess::{BitBoard, Board, Color, Piece, Rank, Square};

use crate::table::WdlTable;
use crate::{Data, Material, SyzygyError, Wdl, MAX_PIECES};

/// A collection of tablebase files that can be probed.
pub struct Tablebase {
    max_pieces: u32,
    wdl: HashMap<Material, WdlTable>,
}

impl Tablebase {
    pub fn new() -> Tablebase {
        Tablebase {
            max_pieces: 2,
            wdl: HashMap::new(),
        }
    }

    /// Load all of the Syzygy tablebase files in the specified directory.
    ///
    /// Syzygy tablebase files have the extension `rtbw` for WDL data and `rtbz` for DTZ data. See
    /// [`Tablebase::load_file`][Tablebase::load_file] for more information.
    pub fn add_directory(&mut self, dir: impl AsRef<Path>) -> Result<(), SyzygyError> {
        for f in std::fs::read_dir(dir)? {
            let f = f?;
            if !f.file_type()?.is_file() {
                continue;
            }
            let path = f.path();
            if path.extension().and_then(|s| s.to_str()) != Some("rtbw") {
                continue;
            }
            self.load_file(path)?;
        }
        Ok(())
    }

    /// Load a Syzygy tablebase file from the file system.
    ///
    /// The non-extension part of the filename is used to determine the material of the tablebase
    /// file, which is information not contained within the Syzygy tablebase file format. It must
    /// be in the standard `K#vK#` format, where `#` is any number of piece characters. If this is
    /// not correct for the file contents, using it may result in panics or incorrect results.
    ///
    /// This memory-maps the file.
    pub fn load_file(&mut self, file: impl AsRef<Path>) -> Result<(), SyzygyError> {
        let path = file.as_ref();

        let material = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or(SyzygyError::UnknownMaterial)?;

        self.load_file_with_material(material, path)
    }

    /// Load a Syzygy tablebase file from the file system.
    ///
    /// The non-extension part of the filename is used to determine the material of the tablebase
    /// file, which is information not contained within the Syzygy tablebase file format. It must
    /// be in the standard `K#vK#` format, where `#` is any number of piece characters. If this is
    /// not correct for the file contents, using it may result in panics or incorrect results.
    ///
    /// This memory-maps the file.
    pub fn load_file_with_material(
        &mut self,
        material: &str,
        file: impl AsRef<Path>,
    ) -> Result<(), SyzygyError> {
        let path = file.as_ref();

        let material: Material = material.parse()?;

        assert!(
            material.count() as usize <= MAX_PIECES,
            "Cannot load tablebase for positions with more than {} pieces",
            MAX_PIECES
        );

        if let Entry::Vacant(entry) = self.wdl.entry(material) {
            let file = std::fs::File::open(path)?;
            let mmap = unsafe { memmap::Mmap::map(&file)? };

            entry.insert(WdlTable::load(Data::File(mmap), material)?);
            self.max_pieces = self.max_pieces.max(material.count() as u32);
        }

        Ok(())
    }

    /// Load a Syzygy tablebase file from static memory.
    ///
    /// The material string must be in the standard `K#vK#` format, where `#` is any number of
    /// piece characters. If this is not correct for the file contents, using it may result in
    /// panics or incorrect results.
    pub fn load_bytes_static(
        &mut self,
        material: &str,
        bytes: &'static [u8],
    ) -> Result<(), SyzygyError> {
        let material: Material = material.parse()?;

        assert!(
            material.count() as usize <= MAX_PIECES,
            "Cannot load tablebase for positions with more than {} pieces",
            MAX_PIECES
        );

        if let Entry::Vacant(entry) = self.wdl.entry(material) {
            entry.insert(WdlTable::load(Data::StaticBytes(bytes), material)?);
            self.max_pieces = self.max_pieces.max(material.count() as u32);
        }
        Ok(())
    }

    /// Load a Syzygy tablebase file from owned memory.
    ///
    /// The material string must be in the standard `K#vK#` format, where `#` is any number of
    /// piece characters. If this is not correct for the file contents, using it may result in
    /// panics or incorrect results.
    pub fn load_bytes_owned(
        &mut self,
        material: &str,
        bytes: Box<[u8]>,
    ) -> Result<(), SyzygyError> {
        let material: Material = material.parse()?;

        assert!(
            material.count() as usize <= MAX_PIECES,
            "Cannot load tablebase for positions with more than {} pieces",
            MAX_PIECES
        );

        if let Entry::Vacant(entry) = self.wdl.entry(material) {
            entry.insert(WdlTable::load(Data::OwnedBytes(bytes), material)?);
            self.max_pieces = self.max_pieces.max(material.count() as u32);
        }
        Ok(())
    }

    /// Returns the number of pieces in the largest Syzygy tablebase file that has been loaded.
    pub fn max_pieces(&self) -> u32 {
        self.max_pieces
    }

    /// Find the WDL value of the specified position, and whether the best move is a capture or
    /// en passant capture.
    ///
    /// Note that due to the way Syzygy tablebases work, the Syzygy tablebase files for subsets
    /// of the material in the specified position may also need to be loaded in order for this
    /// function to return a result.
    pub fn probe_wdl(&self, position: &Board) -> Option<(Wdl, bool)> {
        let v = self.read_wdl(position)?;

        // We need to search the capture moves (See Self::probe_alpha_beta).
        // We also need to know if the position without EP is stalemate, since in that case we
        // need to take the WDL of the best EP capture and completely ignore the TB WDL (draw).
        let their_pieces = position.colors(!position.side_to_move());
        let ep_mask = match position.en_passant() {
            Some(f) => Square::new(f, Rank::Sixth.relative_to(position.side_to_move())).bitboard(),
            None => BitBoard::EMPTY,
        };
        let mut captures = vec![];
        let mut num_moves_without_ep = 0;
        position.generate_moves(|mut mvs| {
            num_moves_without_ep += mvs.len();
            mvs.to &= their_pieces
                | match mvs.piece {
                    Piece::Pawn => ep_mask,
                    _ => BitBoard::EMPTY,
                };
            for mv in mvs {
                let ep = mvs.piece == Piece::Pawn && mv.to.bitboard() == ep_mask;
                if ep {
                    // uncount en passant move
                    num_moves_without_ep -= 1;
                }
                captures.push((mv, ep));
            }
            false
        });

        // The TB provides a lower bound on the WDL unless the position is stalemate without
        // en passant, in which case the lower bound is a loss. Additionally, since we need to know
        // if the best move is a capture when it is better than a draw, it is simpler to initialize
        // alpha to draw even if the tablebase WDL is better than a draw.
        let false_stalemate = num_moves_without_ep == 0 && !captures.is_empty();
        let mut alpha = match false_stalemate {
            true => Wdl::Loss,
            false => Wdl::Draw.min(v),
        };

        let mut best_is_ep = false;
        let mut best_is_capture = false;
        for (mv, ep) in captures {
            let mut new_pos = position.clone();
            new_pos.play_unchecked(mv);
            let v = -self.probe_alpha_beta(&new_pos, Wdl::Loss, -alpha)?;
            if v > alpha {
                best_is_capture = v > Wdl::Draw;
                best_is_ep = ep;
                if v == Wdl::Win {
                    return Some((Wdl::Win, true));
                }
                alpha = v;
            }
        }

        if !false_stalemate && v > alpha {
            Some((v, false))
        } else {
            Some((alpha, best_is_capture || best_is_ep || false_stalemate))
        }
    }

    fn probe_alpha_beta(&self, position: &Board, mut alpha: Wdl, beta: Wdl) -> Option<Wdl> {
        debug_assert!(position.en_passant().is_none());

        // Read the WDL value of the position from the tablebase. This may be worse than the true
        // WDL of the position; if a position has a capture producing a position with the same WDL
        // as this position, then the tablebase can achieve better compression by storing a worse
        // WDL for this position instead.
        let v = self.read_wdl(position)?;
        if v > alpha {
            if v >= beta {
                return Some(v);
            }
            alpha = v;
        }

        // To deal with the above complication, we iterate over capture moves recursively to
        // determine the capture-move WDL, and use that if it is greater than the stored WDL.
        // This is low depth, as tablebase positions do not have very many pieces available for
        // capture, and we further limit the extent of the search by doing alpha-beta pruning.
        let their_pieces = position.colors(!position.side_to_move());
        let mut captures = vec![];
        position.generate_moves(|mut mvs| {
            mvs.to &= their_pieces;
            captures.extend(mvs);
            false
        });

        for mv in captures {
            let mut new_pos = position.clone();
            new_pos.play_unchecked(mv);
            let v = -self.probe_alpha_beta(&new_pos, -beta, -alpha)?;
            if v > alpha {
                if v >= beta {
                    return Some(v);
                }
                alpha = v;
            }
        }

        Some(alpha)
    }

    fn read_wdl(&self, position: &Board) -> Option<Wdl> {
        // Tablebases do not include positions with castle rights
        if position.castle_rights(Color::White).short.is_some()
            || position.castle_rights(Color::White).long.is_some()
            || position.castle_rights(Color::Black).short.is_some()
            || position.castle_rights(Color::Black).long.is_some()
        {
            return None;
        }

        let mut material = Material::default();
        for c in Color::ALL {
            for p in Piece::ALL {
                if p == Piece::King {
                    continue;
                }
                material[(c, p)] = (position.pieces(p) & position.colors(c)).len() as u8;
            }
        }

        if material == Material::default() {
            // KvK
            return Some(Wdl::Draw);
        }

        let color_flip = !material.is_canonical()
            || material.is_symmetric() && position.side_to_move() == Color::Black;

        let material = match color_flip {
            true => material.flip(),
            false => material,
        };

        self.wdl
            .get(&material)
            .map(|table| table.read(position, color_flip))
    }
}
