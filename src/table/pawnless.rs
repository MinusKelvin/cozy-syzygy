use cozy_chess::{Board, Color, File, Piece, Rank, Square};

use crate::constants::{
    BINOMIAL, DIAGONAL, FLIP_DIAGONAL, KK_INDEX, LOWER, OFF_DIAGONAL, TRIANGLE,
};
use crate::pairs::PairsData;
use crate::{ColoredPiece, DataStream, Material, Wdl, MAX_PIECES};

use super::subfactor;

pub struct WdlTable<'data> {
    men: usize,
    encoding_type: EncodingType,
    white_to_move: Table<'data>,
    black_to_move: Option<Table<'data>>,
}

struct Table<'data> {
    pieces: [ColoredPiece; MAX_PIECES],
    norm: [u8; MAX_PIECES],
    factors: [i32; MAX_PIECES],
    pairs_data: PairsData<'data>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum EncodingType {
    Zero,
    Two,
}

impl<'data> WdlTable<'data> {
    pub(crate) fn new(data: &mut DataStream<'data>, material: Material) -> Self {
        let mut encoding_type = EncodingType::Two;
        'outer: for c in Color::ALL {
            for p in Piece::ALL {
                if p == Piece::King {
                    continue;
                }
                if material[(c, p)] == 1 {
                    encoding_type = EncodingType::Zero;
                    break 'outer;
                }
            }
        }
        let enc = encoding_type;

        let men = material.count() as usize;

        let flags = data.read_u8();
        let split = flags & 1 != 0;

        assert_eq!(split, !material.is_symmetric());

        let order = data.read_u8();
        let wtm_order = order & 0xF;
        let btm_order = order >> 4;
        let mut wtm_pieces = [ColoredPiece::WhitePawn; MAX_PIECES];
        let mut btm_pieces = [ColoredPiece::WhitePawn; MAX_PIECES];
        for i in 0..men {
            let p = data.read_u8();
            wtm_pieces[i] = ColoredPiece::decode(p & 0xF).unwrap();
            if split {
                btm_pieces[i] = ColoredPiece::decode(p >> 4).unwrap();
            }
        }

        data.align_to(2);

        let wtm_norm = calculate_norm(men, enc, &wtm_pieces);
        let (wtm_tbsize, wtm_factors) = calculate_factors(men, wtm_order, &wtm_norm, enc);

        let (wtm_pd, wtm_sizes) = PairsData::create(data, wtm_tbsize, true);
        let mut wtm = Table {
            pieces: wtm_pieces,
            norm: wtm_norm,
            factors: wtm_factors,
            pairs_data: wtm_pd,
        };

        let mut btm = split.then(|| {
            let btm_norm = calculate_norm(men, enc, &btm_pieces);
            let (btm_tbsize, btm_factors) = calculate_factors(men, btm_order, &btm_norm, enc);
            let (btm_pd, btm_sizes) = PairsData::create(data, btm_tbsize, true);
            (
                Table {
                    pieces: btm_pieces,
                    norm: btm_norm,
                    factors: btm_factors,
                    pairs_data: btm_pd,
                },
                btm_sizes,
            )
        });

        wtm.pairs_data.index_table = data.read_array(wtm_sizes.index_table_size);
        if let Some((btm, btm_sizes)) = btm.as_mut() {
            btm.pairs_data.index_table = data.read_array(btm_sizes.index_table_size)
        }

        wtm.pairs_data.size_table = data.read_array(wtm_sizes.size_table_size);
        if let Some((btm, btm_sizes)) = btm.as_mut() {
            btm.pairs_data.size_table = data.read_array(btm_sizes.size_table_size)
        }

        data.align_to(64);
        wtm.pairs_data.data = data.read_array(wtm_sizes.data_table_size);
        if let Some((btm, btm_sizes)) = btm.as_mut() {
            data.align_to(64);
            btm.pairs_data.data = data.read_array(btm_sizes.data_table_size)
        }

        WdlTable {
            men,
            encoding_type: enc,
            white_to_move: wtm,
            black_to_move: btm.map(|(pd, _)| pd),
        }
    }

    pub fn read(&self, position: &Board, color_flip: bool) -> Wdl {
        let color_flip = |c: Color| match color_flip {
            true => !c,
            false => c,
        };

        let table = match color_flip(position.side_to_move()) {
            Color::White => &self.white_to_move,
            Color::Black => self.black_to_move.as_ref().unwrap(),
        };

        let mut piece_squares = [Square::A1; MAX_PIECES];

        let mut i = 0;
        while i < self.men {
            let bb = position.pieces(table.pieces[i].piece())
                & position.colors(color_flip(table.pieces[i].color()));
            debug_assert!(!bb.is_empty());
            for sq in bb {
                piece_squares[i] = sq;
                i += 1;
            }
        }

        match table
            .pairs_data
            .lookup(table.index(self.encoding_type, &mut piece_squares[..self.men]))
        {
            0 => Wdl::Loss,
            1 => Wdl::BlessedLoss,
            2 => Wdl::Draw,
            3 => Wdl::CursedWin,
            4 => Wdl::Win,
            _ => unreachable!(),
        }
    }
}

impl Table<'_> {
    fn index(&self, enc: EncodingType, piece_squares: &mut [Square]) -> u64 {
        // We make aggressive use of mirroring here.
        // If the first piece is not in the bottom-left quadrant, it is mirrored there.
        if piece_squares[0].file() > File::D {
            for sq in &mut *piece_squares {
                *sq = sq.flip_file();
            }
        }
        if piece_squares[0].rank() > Rank::Fourth {
            for sq in &mut *piece_squares {
                *sq = sq.flip_rank();
            }
        }

        // Diagonal mirroring
        let to_check = match enc {
            EncodingType::Zero => 3,
            EncodingType::Two => 2,
        };
        for &sq in piece_squares.iter().take(to_check) {
            match OFF_DIAGONAL[sq as usize] {
                -1 => break,
                0 => continue,
                1 => {
                    for sq in &mut *piece_squares {
                        *sq = FLIP_DIAGONAL[*sq as usize];
                    }
                    break;
                }
                _ => unreachable!(),
            }
        }

        let (mut i, mut index) = match enc {
            EncodingType::Zero => {
                let i = (piece_squares[1] > piece_squares[0]) as u64;
                let j = (piece_squares[2] > piece_squares[0]) as u64
                    + (piece_squares[2] > piece_squares[1]) as u64;

                let index = if OFF_DIAGONAL[piece_squares[0] as usize] != 0 {
                    0 * 0
                        + 62 * 63 * TRIANGLE[piece_squares[0] as usize] as u64
                        + 62 * (piece_squares[1] as u64 - i)
                        + (piece_squares[2] as u64 - j)
                } else if OFF_DIAGONAL[piece_squares[1] as usize] != 0 {
                    62 * 63 * 6
                        + 62 * 28 * DIAGONAL[piece_squares[0] as usize] as u64
                        + 62 * LOWER[piece_squares[1] as usize] as u64
                        + (piece_squares[2] as u64 - j)
                } else if OFF_DIAGONAL[piece_squares[2] as usize] != 0 {
                    62 * 63 * 6
                        + 62 * 28 * 4
                        + 28 * 7 * DIAGONAL[piece_squares[0] as usize] as u64
                        + 28 * (DIAGONAL[piece_squares[1] as usize] as u64 - i)
                        + LOWER[piece_squares[2] as usize] as u64
                } else {
                    62 * 63 * 6
                        + 62 * 28 * 4
                        + 28 * 7 * 4
                        + 6 * 7 * DIAGONAL[piece_squares[0] as usize] as u64
                        + 6 * (DIAGONAL[piece_squares[1] as usize] as u64 - i)
                        + (DIAGONAL[piece_squares[2] as usize] as u64 - j)
                };
                (3, index)
            }
            EncodingType::Two => (
                2,
                KK_INDEX[TRIANGLE[piece_squares[0] as usize] as usize][piece_squares[1] as usize]
                    .try_into()
                    .unwrap(),
            ),
        };

        index *= self.factors[0] as u64;

        while i < piece_squares.len() {
            let t = self.norm[i] as usize;
            for j in i..i + t {
                for k in j + 1..i + t {
                    if piece_squares[j] > piece_squares[k] {
                        piece_squares.swap(j, k);
                    }
                }
            }

            let mut s = 0;
            for m in i..i + t {
                let p = piece_squares[m];
                let mut j = 0;
                for l in 0..i {
                    j += (p > piece_squares[l]) as usize;
                }
                s += BINOMIAL[m - i][p as usize - j] as u64;
            }

            index += s * self.factors[i] as u64;
            i += t;
        }

        index
    }
}

fn calculate_norm(
    men: usize,
    enc: EncodingType,
    pieces: &[ColoredPiece; MAX_PIECES],
) -> [u8; MAX_PIECES] {
    let mut norm = [0; MAX_PIECES];

    match enc {
        EncodingType::Zero => norm[0] = 3,
        EncodingType::Two => norm[0] = 2,
    }

    let mut i = norm[0].into();
    while i < men {
        for j in i..men {
            if pieces[i] != pieces[j] {
                break;
            }
            norm[i] += 1;
        }

        i += usize::from(norm[i]);
    }

    norm
}

fn calculate_factors(
    men: usize,
    order: u8,
    norm: &[u8; MAX_PIECES],
    enc: EncodingType,
) -> (usize, [i32; MAX_PIECES]) {
    let mut factors = [0; MAX_PIECES];

    let pivfac = match enc {
        EncodingType::Zero => 31332,
        EncodingType::Two => 462,
    };

    let mut i: usize = norm[0].into();
    let mut f = 1;
    for k in 0.. {
        if k == order {
            factors[0] = f.try_into().unwrap();
            f *= pivfac;
        } else if i < men {
            factors[i] = f.try_into().unwrap();
            f *= subfactor(norm[i].into(), 64 - i);
            i += usize::from(norm[i]);
        } else {
            break;
        }
    }

    (f, factors)
}
