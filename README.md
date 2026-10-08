# whichcrc

Give `whichcrc` a few frames captured from a device and it tells you which CRC protects them: the polynomial, the init value, the bit reflection, the final XOR, and which way round the checksum is stored. If the CRC is one of the 112 in the published catalogue, you get its name. If it isn't in any catalogue, `whichcrc` works the parameters out from the frames.

```
$ whichcrc find '01 04 02 00 e7 f9 7a' '01 04 02 01 02 39 61' '02 04 04 00 e7 01 9c 79 4a' '03 06 00 10 00 2a 08 32'
CRC-16/MODBUS  (catalogue + solver)
  width=16 poly=0x8005 init=0xffff refin=true refout=true xorout=0x0000 check=0x4b37
  CRC is the last 2 bytes, little-endian
  fits all 4 samples; odds of a fluke about 1 in 2^58
```

## Why

Take apart a serial protocol, a radio packet or a save file and you will meet two trailing bytes that are obviously a checksum. The usual next hour goes into an online CRC calculator: pick a CRC-16 variant, try both byte orders, move on to the next variant. The catalogue lists 31 sixteen-bit CRCs. The hour ends badly when the device uses CCITT's polynomial with an init value of its own, because no list of named CRCs has that one.

A CRC is linear over GF(2), so it doesn't have to be searched for. A handful of samples pins down the polynomial with a gcd, and init and xorout with a small linear system. `whichcrc` checks the catalogue first because that is instant and gives the answer a name. Then it solves, so a CRC nobody has catalogued turns up the same way as one everybody uses.

[CRC RevEng](https://reveng.sourceforge.io/) is the reference tool for this job, and its catalogue is where `whichcrc`'s table comes from. `whichcrc` is a smaller take: samples in, an answer with stated odds out, and a plain explanation when the samples can't settle the question.

## Install

You need a Rust toolchain; there are no other dependencies. It was built and tested with Rust 1.97.

```
git clone https://github.com/mangodistro/whichcrc
cd whichcrc
cargo install --path .        # or cargo build --release; the binary lands in target/release/
```

## Usage

```
whichcrc find [options] SAMPLE...            identify the CRC behind some samples
whichcrc calc (-m MODEL | --all) [INPUT...]  compute a CRC
whichcrc list [-w WIDTH] [NAME]              show the built-in catalogue
```

### find

A sample is a whole frame in hex with the CRC in its last bytes, or `MESSAGE=CRC` with the CRC written as a hex number. Paste hex the way other tools print it: `01 03 0a`, `01:03:0a`, `0x01,0x03,0x0a` and `01030a` are the same three bytes. Samples can also come one per line from `-f FILE` or standard input, with `#` starting a comment.

Unless `-w` says otherwise, each frame is read as ending in an 8, 16, 24, 32 or 64-bit CRC, and multi-byte CRCs are tried in both byte orders. Every reading is matched against the catalogue, and handed to the solver when there are enough samples: 3 of one length, or 4 in all. "catalogue + solver" in the first example means both got there independently. The odds line counts every model and every reading tried, not only the one that matched. The [examples](examples/) directory has three captures to try.

**A CRC that no catalogue lists:**

```
$ whichcrc find -f examples/vendor-init.txt
unnamed CRC  (solver)
  width=16 poly=0x1021 init=0x5a17 refin=true refout=true xorout=0x0000 check=0xe911
  CRC is the last 2 bytes, little-endian
  fits all 5 samples; odds of a fluke about 1 in 2^30
  the same CRC exactly: init=0xaa08 xorout=0xf80f (x+1 divides the polynomial, so init and xorout can shift together)
```

The last line is not a hedge. Both parameter sets give the same output for every input; [the first design note](#init-and-xorout-are-not-always-two-numbers) explains why. `--explain` shows how the answer was reached:

```
$ whichcrc find --explain -f examples/vendor-init.txt
...
  how: 3 constraint polynomials (1 same-length, 2 cross-length); their gcd has degree 17
  how: 1 divisor of degree 16 to try
  how: poly 0x1021: 80 equations in 32 unknowns (init, xorout); 1 free, 1 of them from x+1 dividing the poly

note: width 32: samples 1 and 2 have the same message but different CRCs
note: width 64: sample 1 has 7 bytes, this reading needs at least 8
```

**A header the CRC doesn't cover.** `--skip 0-3` tries leaving out 0, 1, 2 and 3 leading bytes; `--tail N` handles bytes after the CRC, such as an end-of-frame marker.

```
$ whichcrc find --skip 0-3 -f examples/sync-header.txt
unnamed CRC  (solver)
  width=16 poly=0x2f15 init=0x41e1 refin=false refout=false xorout=0x5a5a check=0xf77d
  CRC is the last 2 bytes, big-endian
  fits all 6 samples; odds of a fluke about 1 in 2^43
  the leading bytes never change, and a CRC cannot tell those from a different init,
  so this is also:
    leaving out the first byte (aa): init=0xbd30
    leaving out the first 2 bytes (aa 55): init=0x1d0f
```

The example frames were made with the last of these. All three are the same arithmetic on these frames, and more frames with the same header could never tell them apart.

**Samples that can't settle init.** When every frame has the same length, init and xorout can't be separated, and the tool says so rather than guess:

```
$ whichcrc find '01 04 02 00 e7 3a a4' '01 04 02 01 02 fa bf' '02 04 02 00 99 fe 84' '03 04 02 01 2a 83 61'
unnamed CRC  (solver)
  width=16 poly=0x8005 init=0x0000 refin=true refout=true xorout=0xdee7 check=0x65da
  CRC is the last 2 bytes, little-endian
  fits all 4 samples; odds of a fluke about 1 in 2^29
  init and xorout are not pinned down: every sample has the same length, so 2^15 different CRCs fit.
  Other conventional choices that fit:
    init=0x0bad xorout=0x0000 check=0x356b
    init=0xffff xorout=0xdec3 check=0x95f4
    init=0xa982 xorout=0xffff check=0xee94
  A sample of another length would decide between them.
```

Adding one longer frame, `04 06 00 10 00 2a 97 d4`, settles it at `init=0x0bad xorout=0x0000`, which is what made these frames.

**Messages and CRCs given separately**, the way datasheets print them. The number of digits sets the width (four digits, 16 bits) unless `-w` overrides it:

```
$ whichcrc find --text '123456789=29b1' 'hello=d26e'
CRC-16/IBM-3740  (catalogue)
  width=16 poly=0x1021 init=0xffff refin=false refout=false xorout=0x0000 check=0x29b1
  fits both samples; odds of a fluke about 1 in 2^27
```

**Nothing fits.** The exit status is 1 and the reasons go to standard error:

```
$ whichcrc find '01 02 03 04 05 06' 'aa bb cc dd ee ff' '10 20 30 40 50 60 70'
whichcrc: no CRC fits all 3 samples
  tried the catalogue and the solver on 7 readings (widths 8, 16, 24, 32, 64)
  the solver needs at least 3 samples of one length or 4 of mixed lengths; 3 here, 4 needed
  width 64: sample 1 has 6 bytes, this reading needs at least 8
  if the CRC leaves out a header or is followed by more bytes, try --skip or --tail; other widths need -w
```

| option | |
|---|---|
| `-w, --width N[,N...]` | CRC widths to try, 1 to 64 bits. Needed for widths that aren't whole bytes, such as CRC-12 in a 2-byte field |
| `-e, --endian ORDER` | `big`, `little` or `both` (default) |
| `-s, --skip N[-M]` | leading bytes the CRC doesn't cover; a range tries each |
| `-t, --tail N` | bytes after the CRC field |
| `-f, --file FILE` | samples one per line, `-` for standard input |
| `-b, --binary` | each argument is a file holding one raw frame, such as a firmware image with its CRC appended |
| `--text` | the MESSAGE in `MESSAGE=CRC` is text, not hex |
| `--catalogue-only`, `--solve-only` | use only one of the two methods |
| `--explain` | show the solver's working and any notes |

### calc

Computes a CRC by catalogue name, alias or parameter line. Use it to check an implementation, or to reproduce what `find` reported:

```
$ whichcrc calc -m MODBUS --hex '01 04 02 00 e7'
0x7af9
$ whichcrc calc -m 'width=16 poly=0x1021 init=0x5a17 refin=true refout=true xorout=0' --text 123456789
0xe911
$ whichcrc calc --all --text 123456789 | head -3
CRC-3/GSM                 0x4
CRC-3/ROHC                0x6
CRC-4/G-704               0x7
```

The value is the CRC as a number. On the wire, MODBUS sends `0x7af9` low byte first, as `f9 7a`. Input comes from `--hex`, `--text`, file arguments, or standard input. Names are matched without regard to case, and the part after the slash is enough when it is unique (`ibm-3740`). An ambiguous name lists the candidates:

```
$ whichcrc calc -m XZ --text abc
whichcrc: "XZ" could be any of: CRC-32/ISO-HDLC, CRC-64/XZ
```

### list

```
$ whichcrc list -w 5
CRC-5/EPC-C1G2            width=5 poly=0x09 init=0x09 refin=false refout=false xorout=0x00 check=0x00
CRC-5/G-704               width=5 poly=0x15 init=0x00 refin=true refout=true xorout=0x00 check=0x07
CRC-5/USB                 width=5 poly=0x05 init=0x1f refin=true refout=true xorout=0x1f check=0x19
$ whichcrc list crc-32c
CRC-32/ISCSI
  width=32 poly=0x1edc6f41 init=0xffffffff refin=true refout=true xorout=0xffffffff
  check=0xe3069283 residue=0xb798b438
  aliases: CRC-32/BASE91-C, CRC-32/CASTAGNOLI, CRC-32/INTERLAKEN, CRC-32C, CRC-32/NVME
```

`check` is the CRC of the ASCII string `123456789`, the usual way to identify a CRC in documentation.

### Exit status

0 when something was found or computed, 1 when `find` matched nothing, 2 for bad usage or unreadable input.

## How it works

Undo the reflections first: bit-reverse each input byte if `refin`, and the CRC if `refout`. What is left is a plain MSB-first CRC, and for a message M of L bytes it is

```
crc = (M(x)·x^w + init·x^(8L)) mod P(x)  +  xorout
```

over GF(2). Subtract two samples of the same length and init and xorout cancel, so P divides `(M1 + M2)·x^w + crc1 + crc2`, a polynomial you can write down. With three different lengths, init can be eliminated between pairs as well. The gcd of all these polynomials is P times, usually, a few junk factors. Factoring the gcd (square-free split, distinct-degree split, then Cantor–Zassenhaus) and multiplying factors back together gives every candidate P of the right degree. Over GF(2) the splitting map is the trace, a + a² + a⁴ + … + a^(2^(d−1)), which is 0 or 1 modulo each factor of degree d.

For a fixed P the CRC is affine in the bits of init and xorout, so they come out of a linear system: w equations per sample, 2w unknowns, one `u128` per row. Every answer is then checked against the original frames with the ordinary table-driven CRC before it is printed.

The odds line is bookkeeping. n samples of w bits carry n·w bits of evidence. A catalogue match spends log2 of the number of models and readings tried. The solver also spends w bits for every parameter it fitted: 3w for poly, init and xorout, and 2w when init and xorout are tied.

## Design decisions

### init and xorout are not always two numbers

My first version took it as given that samples of more than one length pin down init and xorout separately. A test said so: feed the solver AUTOSAR's published test messages for CRC-16/ARC, expect ARC back and nothing else. The test failed. The solver returned ARC and a second answer, `init=0x8003 xorout=0xc001`, and the second answer was not wrong.

ARC's polynomial is (x+1)(x^15+x+1). Write Q = P/(x+1). Since x ≡ 1 mod (x+1), Q·x^n ≡ Q mod P for every n. So adding Q to the register at the start leaves it exactly Q higher at the end, whatever the message, and adding Q to xorout as well cancels it. The two parameter sets compute the same function on every input of every length. Any polynomial with an even number of terms is divisible by x+1, and that covers 74 of the 112 catalogued models, including 0x8005 and 0x1021, the two most common 16-bit polynomials.

Now the solver splits its leftover freedom in two. Twins are the same CRC written two ways: it prints the catalogued or most conventional form (init 0, then xorout 0, then all-ones values) and lists the twin. Freedom beyond the twins means the samples really can't decide, and the output says that instead. The cost is an extra line on many results that a reader may take for uncertainty, and a preference order for "conventional" that is a judgment call.

### A constant header looks exactly like a different init

The first `--skip` run on frames with a sync header returned three unrelated-looking CRCs: covering the header with init `0x41e1`, skipping one byte with `0xbd30`, skipping two with `0x1d0f`. All three fit, because feeding fixed bytes into the register just lands it on another fixed value, which is what init is. Results that differ only in skip are now folded together when the skipped bytes are identical in every sample and running them through one model's register produces the other's init. Which one the device actually computes can't be learned from the frames. The tool lists every form and leaves the choice to you.

### Report the family, not a guess

With every sample the same length, 2^15 or so different CRCs fit, and picking one would be inventing an answer. The output names a few conventional members and asks for a sample of another length. That costs more text, and a second capture if you want a definite answer.

### Odds, not adjectives

Every match carries the chance that it is a coincidence, after paying for every model, byte order and skip value tried. More readings make a fluke likelier, and the number shows that cost. It assumes the data looks random: frames that are mostly zeros, or that differ in only one byte, make coincidences likelier than the figure says.

### Widths up to 64, polynomials with a +1 term

The engine keeps the register left-aligned in a `u64`, so one table layout serves every width from 1 to 64, including the ones under 8 bits. The solver divides powers of x out of every constraint, which keeps the gcds small but means a polynomial without the +1 term can't be found. No catalogued CRC has one. CRC-82/DARC, the one catalogue entry wider than 64 bits, is left out.

### Eight bytes at a time

The first engine processed a byte per table lookup, and `find -b` on a 50 MB firmware-style image took 22 seconds and 198 MB of memory: each of the 77 byte-width catalogue models ran over the whole file, and the file was copied for every reading. The engine now does slicing-by-8, and readings borrow the input instead of copying it. The same run takes under 5 seconds and the memory is the file itself. The cost is 16 KB of tables per model, built even for a 6-byte frame where one 2 KB table would do.

### Standard library only

Argument parsing, the polynomial type and the linear algebra are all written out here, so building needs nothing beyond `cargo`. The cost is code a crate would have provided, argument handling most of all, and there is no shell completion.

## Limitations

- Messages are whole bytes. A CRC over an 11-bit USB token can't be given as a sample.
- The CRC covers one contiguous run of the frame: some skipped leading bytes, then the message, then the CRC, then some tail bytes.
- The solver doesn't use samples longer than about 16 KB (the catalogue check still does), and gives up when the gcd is above degree 8192. It tries at most 4096 candidate polynomials.
- The CRC field must hold the CRC in its low bits in plain big- or little-endian order. Fields with padding in other places, or CRCs truncated to fewer bits than the register, are out of scope.

## Tests

`cargo test` runs 78 tests. They cover:

- every catalogue entry's published check and residue values, against both the table engine and a bit-at-a-time reference;
- codewords quoted in the catalogue from AUTOSAR, ISO 14443, T10, Bluetooth and other specifications;
- the engine against the bitwise definition at every width from 1 to 64;
- polynomial factoring against known factorisations: x^15 + 1, and CRC-16's 0x8005 as (x+1)(x^15+x+1);
- the solver recovering random models at widths from 1 to 64, and the twin and same-length cases;
- the command line end to end, including the example files.

The frames in the tests and examples were generated by a separate bit-at-a-time implementation of the CRC model, not by this crate. The engine was also checked against Python's `zlib` and `binascii` and Go's `hash/crc32` and `hash/crc64` on 100 KB and 50 MB of random data.

## Credits

The parameters, check and residue values, aliases and test codewords come from Greg Cook's [Catalogue of parametrised CRC algorithms](https://reveng.sourceforge.io/crc-catalogue/). The parameter model is the one from Ross Williams' *A Painless Guide to CRC Error Detection Algorithms* (1993).

## License

MIT. See [LICENSE](LICENSE).
