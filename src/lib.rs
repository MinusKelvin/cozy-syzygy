use cozy_chess::{Color, Piece};

mod constants;
mod table;
mod tablebase;
mod pairs;

const MAX_PIECES: usize = 8;

pub use tablebase::Tablebase;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wdl {
    Loss,
    BlessedLoss,
    Draw,
    CursedWin,
    Win,
}

impl std::ops::Neg for Wdl {
    type Output = Wdl;

    fn neg(self) -> Self::Output {
        match self {
            Wdl::Loss => Wdl::Win,
            Wdl::BlessedLoss => Wdl::CursedWin,
            Wdl::Draw => Wdl::Draw,
            Wdl::CursedWin => Wdl::BlessedLoss,
            Wdl::Win => Wdl::Loss,
        }
    }
}

const CANONICAL_PIECE_ORDER: [Piece; 5] = [
    Piece::Queen,
    Piece::Rook,
    Piece::Bishop,
    Piece::Knight,
    Piece::Pawn,
];

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
struct Material([[u8; 5]; 2]);

impl Material {
    fn is_symmetric(&self) -> bool {
        self.0[0] == self.0[1]
    }

    fn is_canonical(&self) -> bool {
        let white: u8 = self.0[0].iter().sum();
        let black = self.0[1].iter().sum();
        match white.cmp(&black) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Equal => {},
            std::cmp::Ordering::Less => return false,
        }
        for p in CANONICAL_PIECE_ORDER {
            match self[(Color::White, p)].cmp(&self[(Color::Black, p)]) {
                std::cmp::Ordering::Greater => return true,
                std::cmp::Ordering::Equal => {},
                std::cmp::Ordering::Less => return false,
            }
        }
        true // symmetric
    }

    fn flip(self) -> Self {
        Material([self.0[1], self.0[0]])
    }

    fn count(&self) -> u8 {
        self.0.iter().flatten().sum::<u8>() + 2 // 2 kings
    }
}

impl std::ops::Index<(Color, Piece)> for Material {
    type Output = u8;

    fn index(&self, (c, p): (Color, Piece)) -> &u8 {
        &self.0[c as usize][p as usize]
    }
}

impl std::ops::IndexMut<(Color, Piece)> for Material {
    fn index_mut(&mut self, (c, p): (Color, Piece)) -> &mut u8 {
        &mut self.0[c as usize][p as usize]
    }
}

impl std::fmt::Display for Material {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "K")?;
        for p in CANONICAL_PIECE_ORDER {
            for _ in 0..self[(Color::White, p)] {
                write!(f, "{}", char::from(p).to_ascii_uppercase())?;
            }
        }
        write!(f, "vK")?;
        for p in CANONICAL_PIECE_ORDER {
            for _ in 0..self[(Color::Black, p)] {
                write!(f, "{}", char::from(p).to_ascii_uppercase())?;
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ColoredPiece {
    WhitePawn = 1,
    WhiteKnight = 2,
    WhiteBishop = 3,
    WhiteRook = 4,
    WhiteQueen = 5,
    WhiteKing = 6,
    BlackPawn = 9,
    BlackKnight = 10,
    BlackBishop = 11,
    BlackRook = 12,
    BlackQueen = 13,
    BlackKing = 14,
}

impl ColoredPiece {
    fn decode(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::WhitePawn),
            2 => Some(Self::WhiteKnight),
            3 => Some(Self::WhiteBishop),
            4 => Some(Self::WhiteRook),
            5 => Some(Self::WhiteQueen),
            6 => Some(Self::WhiteKing),
            9 => Some(Self::BlackPawn),
            10 => Some(Self::BlackKnight),
            11 => Some(Self::BlackBishop),
            12 => Some(Self::BlackRook),
            13 => Some(Self::BlackQueen),
            14 => Some(Self::BlackKing),
            _ => None,
        }
    }

    fn piece(self) -> Piece {
        match self as usize & 7 {
            1 => Piece::Pawn,
            2 => Piece::Knight,
            3 => Piece::Bishop,
            4 => Piece::Rook,
            5 => Piece::Queen,
            6 => Piece::King,
            _ => unreachable!()
        }
    }

    fn color(self) -> Color {
        match self as usize & 0x8 == 0 {
            true => Color::White,
            false => Color::Black,
        }
    }
}

struct DataStream<'a> {
    read_so_far: usize,
    data: &'a [u8],
}

impl<'a> DataStream<'a> {
    fn new(data: &'a [u8]) -> Self {
        DataStream {
            read_so_far: 0,
            data
        }
    }

    fn align_to(&mut self, bytes: usize) {
        let over = self.read_so_far % bytes;
        if over > 0 {
            self.read_array(bytes - over);
        }
    }

    fn read_u8(&mut self) -> u8 {
        self.read_array(1)[0]
    }

    fn read_u16(&mut self) -> u16 {
        u16::from_le_bytes(self.read_array(2).try_into().unwrap())
    }

    fn read_u32(&mut self) -> u32 {
        u32::from_le_bytes(self.read_array(4).try_into().unwrap())
    }

    fn read_array(&mut self, size: usize) -> &'a [u8] {
        let (a, r) = self.data.split_at(size);
        self.data = r;
        self.read_so_far += size;
        a
    }
}
