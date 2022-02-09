use std::io::Result;
use std::path::Path;

use cozy_chess::{Board, Color, Piece};
use memmap::Mmap;
use ouroboros::self_referencing;

use crate::{ColoredPiece, Material, Wdl, DataStream};

mod pawnless;

#[self_referencing]
pub struct WdlTable {
    data: Mmap,
    #[borrows(data)]
    #[covariant]
    variant: Variant<'this>,
}

enum Variant<'data> {
    Pawnless(pawnless::WdlTable<'data>),
    Pawnful(),
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
                Err(std::io::Error::from(std::io::ErrorKind::Other))
            }
        })
    }

    pub(super) fn read(&self, pos: &Board, color_flip: bool) -> Wdl {
        match self.borrow_variant() {
            Variant::Pawnless(table) => table.read(pos, color_flip),
            Variant::Pawnful() => todo!(),
        }
    }
}
