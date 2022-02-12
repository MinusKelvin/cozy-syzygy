use std::io::Result;
use std::path::Path;

use cozy_chess::{Board, Color, Piece};
use memmap::Mmap;
use ouroboros::self_referencing;

use crate::{Material, Wdl, DataStream};

mod pawnless;
mod pawnful;

#[self_referencing]
pub struct WdlTable {
    data: Mmap,
    #[borrows(data)]
    #[covariant]
    variant: Variant<'this>,
}

enum Variant<'data> {
    Pawnless(pawnless::WdlTable<'data>),
    Pawnful(pawnful::WdlTable<'data>),
}

impl WdlTable {
    pub(super) fn load(path: &Path, material: Material) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { memmap::Mmap::map(&file)? };

        WdlTable::try_new(mmap, |data| {
            let mut data = DataStream::new(data);
    
            if data.read_u32() != 0x5d23e871 {
                return Err(std::io::Error::from(std::io::ErrorKind::Other));
            }
    
            let wpawns = material[(Color::White, Piece::Pawn)];
            let bpawns = material[(Color::Black, Piece::Pawn)];
    
            if wpawns + bpawns == 0 {
                Ok(Variant::Pawnless(pawnless::WdlTable::new(&mut data, material)))
            } else {
                Ok(Variant::Pawnful(pawnful::WdlTable::new(&mut data, material)))
            }
        })
    }

    pub(super) fn read(&self, pos: &Board, color_flip: bool) -> Wdl {
        match self.borrow_variant() {
            Variant::Pawnless(table) => table.read(pos, color_flip),
            Variant::Pawnful(table) => table.read(pos, color_flip),
        }
    }
}

fn subfactor(k: usize, n: usize) -> usize {
    let mut f = n;
    let mut l = 1;
    for i in 1..k {
        f *= n - i;
        l *= i + 1;
    }

    f / l
}
