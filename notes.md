# Syzygy Tablebase Format Notes

You must know the material of the table to read these files correctly.

## WDL

The file starts with the following header:
```
magic: [71, E8, 23, 5D]
flags: u8
```

`flags & 1` indicates whether the table is "split".
`flags & 2` indicates whether the table has 4 files or 1 (?).

If the material in this table does not contain pawns, then the pawnless format
is used. Otherwise, the pawnful format is used.

When looking up a position, it is possible that the color-flipped position
needs to be queried instead. If white has fewer pieces than black, use the
color-flipped position. If the material is equal and the side to move is black,
use the color-flipped position. Otherwise, in the order queen, rook, bishop,
knight, pawn, if white has more of that piece than black, use the normal
position, and if black has more of that piece than white, use the color-flipped
position.

### Common stuff

#### Pairs struct

```
flags: u8
blocksize: u8
idxbits: u8
extra_blocks: u8
real_num_blocks: u32
max_len: u8
min_len: u8
offsets: [u16; h] where h = max_len - min_len + 1
num_syms: u16
sympat: [u8; 3 * num_syms]
```

Special case: if `flags & 0x80` is set, then the struct is only 2 bytes long. If
the table is WDL, then every position in the table has the value of the second
byte as its WDL.

If `num_syms` is odd, then there is a padding byte following the structure.

There are three derived sizes:
- `size[0] = 6 * num_indices` where
  `num_indices = (tb_size + (1 << idxbits) - 1) >> idxbits`
- `size[1] = 2 * num_blocks` where `num_blocks = real_num_blocks + extra_blocks`
- `size[2] = (1 << blocksize) * real_num_blocks`

(`num_indices` is just `tb_size >> idxbits` but add 1 if any 1s are shifted out)

There are two derived tables: `symlen: [u8; num_syms]` and `base: [u64; h]`.
These seem very complicated, based on the decompress_pairs routine.

#### Decompress pairs, or a better name, lookup(index)

First, the index is broken down into two parts, the `lit_index` which is the
lower `index_bits` bits, and the `main_index` which is the upper bits shifted
down. The `lit_index` has `2^(index_bits-1)` subtracted from it, putting it in
the same range as a twos-complement `index_bits`-bit integer.

The `main_index` is used to index into the index table to find the block, as
well as another offset to apply to `lit_index`.

The next task is to make `lit_index` point inside of the correct block. If it is
negative, we move to the lower block and add its size to `lit_index`. If it is
larger than the current block's size, we subtract the current block's size and
move to the next higher block.

At this point, we have identified where in the data table to start from. We step
through the table, uh, somehow, until `lit_index` lands inside the relevant
sym entry (could this be some kind of palette compression?).

Once the correct `sym` entry is identified, we follow some path using the
`sympat` table (sympat = symbol path, maybe? symbol pattern?) until we find an
entry with length zero.

The result of the lookup is the least significant byte of that entry.

### Pawnless Tables

#### File Data

Pawnless tables have two encoding types (`0` and `2`) depending on the number of
lone piece types. The encoding type is `0` if there are 3 or more piece types
with only 1 of them on the board, otherwise it is encoding type `2`. This is not
stored in the file.

We then have the following structure:
```
order: u8
pieces: [u8; MEN]
```
The low-order 4 bits of each byte contain the data for the white-to-move table,
and the high-order 4 bits contain the data for the black-to-move table.

Each element of `pieces` are encoded as `piece_type | color` where `piece_type`
is 1=pawn, 2=knight, 3=bishop, 4=rook, 5=queen, 6=king and `color` is 0=white,
8=black. Identical pieces are always consecutive.
The `norm` and `factor` parallel arrays are calculated from this, as well as the
`tb_size`.

If the file up to this point has used an odd number of bytes, then one padding
byte is inserted.

If `split` flag from the header was set, that means that there is a
black-to-move table. Each piece the black-to-move data directly follows that
same piece of white-to-move data.

Next, we have the pairs struct for white-to-move. Then, the index table of size
`size[0]` bytes. Then the size table of size `size[1]` bytes, and finally the
data of size `size[2]` bytes.

#### Reading

Use the data tables related to the current side-to-move.

We first build an array of squares parallel to the `pieces` array. Identical
pieces have their squares ordered as the lowest-left square first.

We then apply mirroring rules to exploit symmetry.

1. If the first piece is on the right half of the board, mirror the position
   horizontally.
2. If the first piece is on the top half of the board, mirror the position
   vertically.
3. Locate the first piece off of the A1-H8 diagonal. If it is in the first
   3 (encoding type = 0) or 2 (encoding type = 2) pieces, and that piece is
   above the diagonal (rank > file), then mirror the position across the A1-H8
   diagonal.

See lines 625-663 of `tbcore.c` for how to compute the index. I will explain
what on earth is going on here with pictures at another time.

At this point in the code, it checks if the table all has the same WDL value
and returns that. I don't see why this couldn't be moved much earlier.

Next, we run the pairs data lookup routine with the specified index. 0 = loss,
1 = blessed loss, 2 = draw, 3 = cursed win, 4 = win.
