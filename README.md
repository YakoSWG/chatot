# Chatot

Text archive decoder and encoder for Generation IV Pokémon games.
Chatot allows you to decode binary text archives used in Pokémon games into human-readable text files, and encode text files back into binary archives.
Decoding a vanilla text archive and then re-encoding it will produce an identical binary file.

## Usage

Chatot provides three main commands: `decode`, `encode`, and `format` (not yet implemented).

### Global Options

All commands require:
- `-m, --charmap <PATH>`: Path to custom character map file (required)

### Commands

#### Decode

Decrypt and decode binary text archives to text or json files.

```bash
chatot decode -m <CHARMAP> [INPUT] [OUTPUT] [OPTIONS]
```

**Input Options** (choose one):
- `-b, --archive <PATH>...`: Path(s) to binary text archive file(s)
- `-a, --archive-dir <PATH>`: Directory containing archive files

**Output Options** (choose one):
- `-t, --txt <PATH>...`: Path(s) to output text file(s)
- `-d, --text-dir <PATH>`: Directory for output text files

**Additional Options**:
- `-j, --json`: Read from JSON format
- `-l, --lang <CODE>`: Language code for JSON input (default: `en_US`, requires `--json`)
- `-n, --newer`: Process only files newer than existing outputs
- `--msgenc`: Use msgenc tool format for decoding messages. Usually you should only use this when encoding messages already in msgenc format.

**Examples**:

```bash
# Decode a single archive to a text file
chatot decode -m charmap.json -b archive.bin -t output.txt

# Decode multiple archives
chatot decode -m charmap.json -b archive1.bin archive2.bin -t output1.txt output2.txt

# Decode all archives from a directory to an output directory
chatot decode -m charmap.json -a input_dir/ -d output_dir/

# Decode only newer files with msgenc format
chatot decode -m charmap.json -a input_dir/ -d output_dir/ -n --msgenc
```

#### Encode

Encrypt and encode text files to binary text archives.

```bash
chatot encode -m <CHARMAP> [INPUT] [OUTPUT] [OPTIONS]
```

**Input Options** (choose one):
- `-t, --txt <PATH>...`: Path(s) to text file(s)
- `-d, --text-dir <PATH>`: Directory containing text files

**Output Options** (choose one):
- `-b, --archive <PATH>...`: Path(s) to output binary archive file(s)
- `-a, --archive-dir <PATH>`: Directory for output archive files

**Additional Options**:
- `-j, --json`: Write to JSON format
- `-l, --lang <CODE>`: Language code for JSON output (default: `en_US`, requires `--json`)
- `-n, --newer`: Process only files newer than existing outputs
- `--msgenc`: Use msgenc tool format for encoding messages

**Examples**:

```bash
# Encode a single text file to an archive
chatot encode -m charmap.json -t input.txt -b output.bin

# Encode multiple text files
chatot encode -m charmap.json -t text1.txt text2.txt -b archive1.bin archive2.bin

# Encode all text files from a directory to an output directory
chatot encode -m charmap.json -d input_dir/ -a output_dir/

# Encode with JSON format
chatot encode -m charmap.json -d input_dir/ -a output_dir/ -j -l en_US
```

#### Format

This command is currently **not implemented**. 
The idea is that this would automatically insert the proper line breaks where they are required based on the character map and the text box width limitations of the game.

```bash
chatot format -m <CHARMAP> [INPUT] [OPTIONS]
```

## Building

```bash
cargo build --release
```

The compiled binary will be available at `target/release/chatot`.

## Character Map

All commands require a character map file in JSON format. This file defines the mapping between binary values and text characters specific to Generation IV Pokémon games.
You can download the default character map from this repository.
Keep in mind that the game only supports characters already defined in the character map, adding custom characters would require modifying the game itself.
You can freely add aliases for existing characters in the character map to make text editing easier however.