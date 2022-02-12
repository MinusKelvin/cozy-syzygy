use cozy_chess::{Board, Color, File, Piece, Square};

use crate::constants::{BINOMIAL, FILE_TO_FILE, FLAP, PAWN_FACTOR, PAWN_INDEX, PAWN_TWIST};
use crate::pairs::PairsData;
use crate::{ColoredPiece, DataStream, Material, Wdl, MAX_PIECES};

use super::subfactor;

pub struct WdlTable<'data> {
    men: usize,
    white_pawns: usize,
    black_pawns: usize,
    // todo: refactor
    tables: [[Option<Table<'data>>; 4]; 2],
}

struct Table<'data> {
    pieces: [ColoredPiece; MAX_PIECES],
    norm: [u8; MAX_PIECES],
    factors: [usize; MAX_PIECES],
    pairs_data: PairsData<'data>,
}

impl<'data> WdlTable<'data> {
    pub(crate) fn new(data: &mut DataStream<'data>, material: Material) -> Self {
        let men = material.count() as usize;

        let flags = data.read_u8();
        let split = flags & 1 != 0;
        let files = match flags & 2 != 0 {
            true => 4,
            false => 1,
        };

        assert_eq!(split, !material.is_symmetric());

        let mut white_pawns = material[(Color::White, Piece::Pawn)];
        let mut black_pawns = material[(Color::Black, Piece::Pawn)];
        if white_pawns == 0 || black_pawns != 0 && black_pawns < white_pawns {
            std::mem::swap(&mut white_pawns, &mut black_pawns);
        }
        let black_has_pawns = black_pawns > 0;

        let mut wtm_pieces = [[ColoredPiece::WhitePawn; MAX_PIECES]; 4];
        let mut btm_pieces = [[ColoredPiece::WhitePawn; MAX_PIECES]; 4];
        let mut wtm_tb_sizes = [0; 4];
        let mut btm_tb_sizes = [0; 4];
        let mut wtm_norm = [[0; MAX_PIECES]; 4];
        let mut btm_norm = [[0; MAX_PIECES]; 4];
        let mut wtm_factor = [[0; MAX_PIECES]; 4];
        let mut btm_factor = [[0; MAX_PIECES]; 4];

        for f in 0..files {
            let order = data.read_u8();
            let order2 = match black_has_pawns {
                true => data.read_u8(),
                false => 0xFF,
            };
            let pieces = data.read_array(men);

            for i in 0..men {
                wtm_pieces[f][i] = ColoredPiece::decode(pieces[i] & 0xF).unwrap();
                if split {
                    btm_pieces[f][i] = ColoredPiece::decode(pieces[i] >> 4).unwrap();
                }
            }

            wtm_norm[f] = calculate_norm(white_pawns, black_pawns, men, &wtm_pieces[f]);
            let (tb_size, factors) =
                calculate_factors(&wtm_norm[f], men, order & 0xF, order2 & 0xF, f);
            wtm_tb_sizes[f] = tb_size;
            wtm_factor[f] = factors;

            if split {
                btm_norm[f] = calculate_norm(white_pawns, black_pawns, men, &btm_pieces[f]);
                let (tb_size, factors) =
                    calculate_factors(&btm_norm[f], men, order >> 4, order2 >> 4, f);
                btm_tb_sizes[f] = tb_size;
                btm_factor[f] = factors;
            }
        }

        if files == 1 {
            // skip the pieces data for the next 3 files, they don't exist
            data.read_array(
                3 * match black_has_pawns {
                    true => men + 2,
                    false => men + 1,
                },
            );
        }

        data.align_to(2);

        // yike
        let mut tables = [[(); 4]; 2].map(|a| a.map(|_| None));
        let mut sizes = [[None; 4]; 2];

        for f in 0..files {
            let (pairs_data, s) = PairsData::create(data, wtm_tb_sizes[f], true);
            tables[0][f] = Some(Table {
                pieces: wtm_pieces[f],
                norm: wtm_norm[f],
                factors: wtm_factor[f],
                pairs_data,
            });
            sizes[0][f] = Some(s);
            if split {
                let (pairs_data, s) = PairsData::create(data, btm_tb_sizes[f], true);
                tables[1][f] = Some(Table {
                    pieces: btm_pieces[f],
                    norm: btm_norm[f],
                    factors: btm_factor[f],
                    pairs_data,
                });
                sizes[1][f] = Some(s);
            }
        }

        for f in 0..files {
            tables[0][f].as_mut().unwrap().pairs_data.index_table =
                data.read_array(sizes[0][f].as_ref().unwrap().index_table_size);
            if split {
                tables[1][f].as_mut().unwrap().pairs_data.index_table =
                    data.read_array(sizes[1][f].as_ref().unwrap().index_table_size);
            }
        }

        for f in 0..files {
            tables[0][f].as_mut().unwrap().pairs_data.size_table =
                data.read_array(sizes[0][f].as_ref().unwrap().size_table_size);
            if split {
                tables[1][f].as_mut().unwrap().pairs_data.size_table =
                    data.read_array(sizes[1][f].as_ref().unwrap().size_table_size);
            }
        }

        for f in 0..files {
            data.align_to(64);
            tables[0][f].as_mut().unwrap().pairs_data.data =
                data.read_array(sizes[0][f].as_ref().unwrap().data_table_size);
            if split {
                data.align_to(64);
                tables[1][f].as_mut().unwrap().pairs_data.data =
                    data.read_array(sizes[1][f].as_ref().unwrap().data_table_size);
            }
        }

        WdlTable {
            tables,
            men,
            white_pawns: white_pawns as usize,
            black_pawns: black_pawns as usize,
        }
    }

    pub fn read(&self, pos: &Board, color_flip: bool) -> Wdl {
        let flip_color = |c: Color| match color_flip {
            true => !c,
            false => c,
        };
        let flip_rank = |sq: Square| match color_flip {
            true => sq.flip_rank(),
            false => sq,
        };

        let mut piece_squares = [Square::A1; MAX_PIECES];

        let k = self.tables[0][0].as_ref().unwrap().pieces[0];
        let mut i = 0;
        let bb = pos.pieces(k.piece()) & pos.colors(flip_color(k.color()));
        for sq in bb {
            piece_squares[i] = flip_rank(sq);
            i += 1;
        }

        let f = pawn_file(self.white_pawns, &mut piece_squares);
        let table = self.tables[flip_color(pos.side_to_move()) as usize][f]
            .as_ref()
            .unwrap();

        while i < self.men {
            let bb = pos.pieces(table.pieces[i].piece())
                & pos.colors(flip_color(table.pieces[i].color()));
            for sq in bb {
                piece_squares[i] = flip_rank(sq);
                i += 1;
            }
        }

        match table.pairs_data.lookup(table.index(
            self.white_pawns,
            self.black_pawns,
            &mut piece_squares[..self.men],
        )) {
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
    fn index(&self, white_pawns: usize, black_pawns: usize, piece_squares: &mut [Square]) -> u64 {
        if piece_squares[0].file() > File::D {
            for sq in &mut *piece_squares {
                *sq = sq.flip_file();
            }
        }

        for i in 1..white_pawns {
            for j in i + 1..black_pawns {
                if PAWN_TWIST[piece_squares[i] as usize] < PAWN_TWIST[piece_squares[j] as usize] {
                    piece_squares.swap(i, j);
                }
            }
        }

        let t = white_pawns - 1;
        let mut index = PAWN_INDEX[t][FLAP[piece_squares[0] as usize] as usize] as u64;
        for i in (0..t).rev() {
            index += BINOMIAL[t - 1][PAWN_TWIST[piece_squares[i] as usize] as usize] as u64;
        }
        index *= self.factors[0] as u64;

        let mut i = white_pawns;
        let t = white_pawns + black_pawns;
        if t > i {
            for j in i..t {
                for k in j + 1..t {
                    if piece_squares[j] > piece_squares[k] {
                        piece_squares.swap(j, k);
                    }
                }
            }

            let mut s = 0;
            for m in i..t {
                let sq = piece_squares[m];
                let mut j = 0;
                for k in 0..i {
                    if sq > piece_squares[k] {
                        j += 1;
                    }
                }
                s += BINOMIAL[m - i][sq as usize - j - 8] as u64;
            }

            index += s * self.factors[i] as u64;
            i = t;
        }

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
                let sq = piece_squares[m];
                let mut j = 0;
                for k in 0..i {
                    if sq > piece_squares[k] {
                        j += 1;
                    }
                }
                s += BINOMIAL[m - i][sq as usize - j] as u64;
            }

            index += s * self.factors[i] as u64;
            i += t;
        }

        index
    }
}

fn calculate_norm(
    white_pawns: u8,
    black_pawns: u8,
    men: usize,
    pieces: &[ColoredPiece; MAX_PIECES],
) -> [u8; MAX_PIECES] {
    let mut result = [0; MAX_PIECES];

    result[0] = white_pawns;
    if black_pawns > 0 {
        result[result[0] as usize] = black_pawns;
    }

    let mut i = (white_pawns + black_pawns) as usize;
    while i < men {
        for _ in (i..men).take_while(|&j| pieces[i] == pieces[j]) {
            result[i] += 1;
        }
        i += result[i] as usize;
    }

    result
}

fn calculate_factors(
    norm: &[u8; MAX_PIECES],
    men: usize,
    order: u8,
    order2: u8,
    file: usize,
) -> (usize, [usize; MAX_PIECES]) {
    let mut i = norm[0] as usize;
    if order2 < 0xF {
        i += norm[i] as usize;
    }

    let mut factor = [0; MAX_PIECES];

    let mut f: usize = 1;
    for k in 0.. {
        if k == order {
            factor[0] = f;
            f *= PAWN_FACTOR[norm[0] as usize - 1][file] as usize;
        } else if k == order2 {
            factor[norm[0] as usize] = f;
            f *= subfactor(norm[norm[0] as usize] as usize, 48 - norm[0] as usize) as usize;
        } else if i < men {
            factor[i] = f;
            f *= subfactor(norm[i] as usize, 64 - i) as usize;
            i += norm[i] as usize;
        } else {
            break;
        }
    }

    (f, factor)
}

fn pawn_file(white_pawns: usize, piece_squares: &mut [Square; MAX_PIECES]) -> usize {
    for i in 0..white_pawns {
        if FLAP[piece_squares[0] as usize] > FLAP[piece_squares[i] as usize] {
            piece_squares.swap(0, i);
        }
    }
    FILE_TO_FILE[piece_squares[0].file() as usize] as usize
}
