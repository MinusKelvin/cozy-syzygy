use crate::DataStream;

pub struct PairsData<'data> {
    index_bits: usize,
    min_len: usize,
    block_size: usize,
    offsets: &'data [u8],
    sympat: &'data [u8],
    symlen: Vec<u8>,
    base: Vec<u64>,
    // Filled in elsewhere
    pub index_table: &'data [u8],
    pub size_table: &'data [u8],
    pub data: &'data [u8],
}

#[derive(Default, Debug, Copy, Clone)]
pub struct Sizes {
    pub index_table_size: usize,
    pub size_table_size: usize,
    pub data_table_size: usize,
}

impl<'data> PairsData<'data> {
    pub(crate) fn create(data: &mut DataStream<'data>, tb_size: usize, wdl: bool) -> (Self, Sizes) {
        let flags = data.read_u8();
        if flags & 0x80 != 0 {
            let min_len = data.read_u8() as usize;
            return (
                PairsData {
                    index_bits: 0,
                    min_len: match wdl {
                        true => min_len,
                        false => 0,
                    },
                    block_size: 0,
                    symlen: vec![],
                    base: vec![],
                    offsets: &[],
                    index_table: &[],
                    size_table: &[],
                    data: &[],
                    sympat: &[],
                },
                Sizes {
                    index_table_size: 0,
                    size_table_size: 0,
                    data_table_size: 0,
                },
            );
        }

        let block_size = data.read_u8() as usize;
        let index_bits = data.read_u8() as usize;
        let extra_blocks = data.read_u8() as usize;
        let real_num_blocks = data.read_u32() as usize;
        let num_blocks = real_num_blocks + extra_blocks;
        let max_len = data.read_u8() as usize;
        let min_len = data.read_u8() as usize;
        let h = max_len - min_len + 1;
        let offsets = data.read_array(2 * h);
        let num_syms = data.read_u16() as usize;
        let sympat = data.read_array(3 * num_syms);
        data.align_to(2);

        let num_indices = (tb_size + (1 << index_bits) - 1) >> index_bits;

        let mut tmp = vec![false; num_syms];
        let mut symlen = vec![0; num_syms];
        for i in 0..num_syms {
            if !tmp[i] {
                calculate_symlen(&mut symlen, sympat, i, &mut tmp);
            }
        }

        let mut base = vec![0; h];
        for i in (0..h - 1).rev() {
            let off_i = u16::from_le_bytes(offsets[2 * i..2 * i + 2].try_into().unwrap());
            let off_ip1 = u16::from_le_bytes(offsets[2 * i + 2..2 * i + 4].try_into().unwrap());
            base[i] = (base[i + 1] + off_i as u64 - off_ip1 as u64) / 2;
        }
        for i in 0..h {
            base[i] <<= 64 - (min_len + i);
        }

        // offsets is shifted back by min_len here in the C, but that's obviously terrible in Rust,
        // so we'll just have to remember to subtract min_len before we access it later.

        (
            PairsData {
                index_bits,
                min_len,
                block_size,
                offsets,
                sympat,
                symlen,
                base,
                // these need to be filled in later by the caller
                index_table: &[],
                size_table: &[],
                data: &[],
            },
            Sizes {
                index_table_size: 6 * num_indices,
                size_table_size: 2 * num_blocks,
                data_table_size: (1 << block_size) * real_num_blocks,
            },
        )
    }

    pub fn lookup(&self, index: u64) -> u8 {
        if self.index_bits == 0 {
            return self.min_len as u8;
        }

        let main_index = (index >> self.index_bits) as usize;
        let index_bits_mask = (1 << self.index_bits) - 1;
        let mut lit_index = (index & index_bits_mask) as i64 - (1 << self.index_bits - 1);

        let mut block = u32::from_le_bytes(
            self.index_table[6 * main_index..6 * main_index + 4]
                .try_into()
                .unwrap(),
        ) as usize;

        lit_index += u16::from_le_bytes(
            self.index_table[6 * main_index + 4..6 * main_index + 6]
                .try_into()
                .unwrap(),
        ) as i64;

        let size_table =
            |i| u16::from_le_bytes(self.size_table[2 * i..2 * i + 2].try_into().unwrap());

        if lit_index < 0 {
            while lit_index < 0 {
                block -= 1;
                lit_index += size_table(block) as i64 + 1;
            }
        } else {
            while lit_index > size_table(block) as i64 {
                lit_index -= size_table(block) as i64 + 1;
                block += 1;
            }
        }

        let mut ptr = &self.data[block << self.block_size..];

        let offset = |l: usize| {
            u16::from_le_bytes(
                self.offsets[2 * (l - self.min_len)..2 * (l - self.min_len + 1)]
                    .try_into()
                    .unwrap(),
            )
        };
        let base = |l: usize| self.base[l - self.min_len];

        let mut code = u64::from_be_bytes(ptr[0..8].try_into().unwrap());
        ptr = &ptr[8..];
        let mut bitcount = 0;
        let mut sym = loop {
            let mut l = self.min_len;
            while base(l) > code {
                l += 1;
            }
            let sym = offset(l) as usize + (code - base(l) >> 64 - l) as usize;
            if lit_index < self.symlen[sym] as i64 + 1 {
                break sym;
            }
            lit_index -= self.symlen[sym] as i64 + 1;
            code <<= l;
            bitcount += l;
            if bitcount >= 32 {
                bitcount -= 32;
                code |= (u32::from_be_bytes(ptr[0..4].try_into().unwrap()) as u64) << bitcount;
                ptr = &ptr[4..];
            }
        };

        while self.symlen[sym] != 0 {
            let w = read_u24(self.sympat[3 * sym..3 * sym + 3].try_into().unwrap()) as usize;
            let s1 = w & 0xFFF;
            if lit_index < self.symlen[s1] as i64 + 1 {
                sym = s1;
            } else {
                lit_index -= self.symlen[s1] as i64 + 1;
                sym = w >> 12;
            }
        }

        return self.sympat[3 * sym];
    }
}

fn calculate_symlen(symlen: &mut [u8], sympat: &[u8], s: usize, tmp: &mut [bool]) {
    let w = read_u24(sympat[3 * s..3 * s + 3].try_into().unwrap()) as usize;
    let s2 = w >> 12;
    if s2 == 0xFFF {
        symlen[s] = 0;
    } else {
        let s1 = w & 0xFFF;
        if !tmp[s1] {
            calculate_symlen(symlen, sympat, s1, tmp);
        }
        if !tmp[s2] {
            calculate_symlen(symlen, sympat, s2, tmp);
        }
        symlen[s] = symlen[s1] + symlen[s2] + 1;
    }
    tmp[s] = true;
}

fn read_u24(data: [u8; 3]) -> u32 {
    u32::from_le_bytes([data[0], data[1], data[2], 0])
}
