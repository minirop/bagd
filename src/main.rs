use clap::Parser;
use clap::Subcommand;
use serde::Deserialize;
use sha1::Digest;
use sha1::Sha1;
use std::fs;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process;
use std::{collections::HashMap, fmt::Display, fs::File};

#[derive(Debug, Parser)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialise the project
    Init,
    /// Update the project (similar to calling clean, linker and split)
    Update,
    /// Generate the linker script
    Linker,
    /// Generate the makefile
    Makefile,
    /// Export the missing parts of the rom
    Split,
    /// Build the rom
    Build,
    /// Clean build artifacts
    Clean,
}

#[derive(Debug, Copy, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Format {
    Asm,
    C,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum Section {
    #[default]
    Text,
    Rodata,
}

impl Display for Section {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Section::Text => write!(f, ".text"),
            Section::Rodata => write!(f, ".rodata"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Segment {
    name: String,
    format: Format,
    #[serde(default)]
    section: Section,
    address: u32,
    size: Option<u32>,
}

#[derive(Debug)]
struct Size(u32);

impl Display for Size {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 > 1_000_000 {
            write!(f, "{}M", self.0 / 1_000_000)
        } else if self.0 > 1_000 {
            write!(f, "{}K", self.0 / 1_000)
        } else {
            todo!()
        }
    }
}

#[derive(Debug, Deserialize)]
struct Rom {
    address: u32,
    #[serde(with = "unit_parser")]
    size: u32,
    segments: Vec<Segment>,
}

#[derive(Debug, Deserialize)]
struct Symbol {
    name: String,
    address: u32,
}

#[derive(Debug, Deserialize)]
struct WRam {
    address: u32,
    size: String,
    symbols: Vec<Symbol>,
}

#[derive(Debug, Deserialize)]
struct Gba {
    name: String,
    sha1: String,
    entry: String,
    rom: Rom,
    iwram: Option<WRam>,
    ewram: Option<WRam>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let yaml = std::fs::read_to_string("project.yml").unwrap();
    let gba: Gba = yaml_serde::from_str(&yaml)?;

    match args.command {
        Commands::Build => build_project(),
        Commands::Clean => clean_project(&gba),
        Commands::Init => init_project(&gba),
        Commands::Linker => ldscript_write(&gba),
        Commands::Makefile => makefile_write(&gba),
        Commands::Split => missing_asm_write(&gba),
        Commands::Update => update_project(&gba),
    }
}

fn ldscript_write(gba: &Gba) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create("ldscript.txt")?;

    writeln!(
        file,
        r#"OUTPUT_ARCH(arm)

MEMORY
{{
    rom   : ORIGIN = 0x{:08X}, LENGTH = {}"#,
        gba.rom.address,
        Size(gba.rom.size)
    )?;
    if let Some(iwram) = &gba.iwram {
        writeln!(
            file,
            "    iwram : ORIGIN = 0x{:08X}, LENGTH = {}",
            iwram.address, iwram.size
        )?;
    }
    if let Some(ewram) = &gba.ewram {
        writeln!(
            file,
            "    ewram : ORIGIN = 0x{:08X}, LENGTH = {}",
            ewram.address, ewram.size
        )?;
    }

    writeln!(
        file,
        r#"}}

__text_start  = ORIGIN(rom);"#
    )?;
    if gba.iwram.is_some() {
        writeln!(file, "__iwram_start = ORIGIN(iwram);")?;
    }
    if gba.ewram.is_some() {
        writeln!(file, "__ewram_start = ORIGIN(ewram);")?;
    }

    writeln!(
        file,
        r#"
ENTRY({})

SECTIONS
{{
    ROM __text_start :
    ALIGN(4)
    {{"#,
        gba.entry
    )?;

    let mut last_size = None;
    let mut current_address = 0;

    let mut segments = HashMap::new();
    for segment in &gba.rom.segments {
        let name = segment.name.as_str();

        if let Some(size) = last_size {
            if current_address + size < segment.address {
                writeln!(
                    file,
                    "        asm/rom_{:06X}.o(.text);",
                    current_address + size
                )?;
            }
        }

        current_address = segment.address;

        let section = &segment.section;

        write!(file, "        ")?;
        match segment.format {
            Format::Asm => writeln!(file, "asm/{name}.o({section});"),
            Format::C => writeln!(file, "src/{name}.o({section});"),
        }?;

        last_size = segment.size;

        segments.insert(name, segment.format);
    }

    let rom_size = 0x800000;
    if let Some(size) = last_size {
        if current_address + size < rom_size {
            writeln!(
                file,
                "        asm/rom_{:06X}.o(.text);",
                current_address + size
            )?;
        }
    }

    let segments = segments;

    writeln!(file, "    }} = 0")?;

    if let Some(iwram) = &gba.iwram {
        wram_write(&mut file, iwram, "iwram", &segments)?;
    }

    if let Some(ewram) = &gba.ewram {
        wram_write(&mut file, ewram, "ewram", &segments)?;
    }

    writeln!(
        file,
        r#"
    /* Discard everything not specifically mentioned above. */
    /DISCARD/ :
    {{
        *(*);
    }}
}}"#
    )?;

    Ok(())
}

fn wram_write(
    file: &mut File,
    wram: &WRam,
    name: &str,
    segments: &HashMap<&str, Format>,
) -> Result<(), Box<dyn std::error::Error>> {
    let name_uppercase = name.to_ascii_uppercase();
    writeln!(
        file,
        r#"
    {name_uppercase} __{name}_start (NOLOAD) :
    ALIGN(4)
    {{"#
    )?;

    for symbol in &wram.symbols {
        let name = symbol.name.as_str();
        let address = symbol.address;

        write!(file, "        . = 0x{:06X}; ", address)?;
        if let Some(format) = segments.get(name) {
            match format {
                Format::Asm => writeln!(file, "asm/{name}.o(.bss);"),
                Format::C => writeln!(file, "src/{name}.o(.bss);"),
            }?;
        } else {
            writeln!(file, "{name} = .;")?;
        }
    }

    writeln!(file, "    }}")?;

    Ok(())
}

fn makefile_write(gba: &Gba) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create("Makefile")?;
    let name = &gba.name;

    writeln!(
        file,
        "#### Tools ####

AGBCC    := /home/minirop/git/fireemblem8u/tools/agbcc
CC1      := $(AGBCC)/bin/old_agbcc
CC1_OLD  := $(AGBCC)/bin/old_agbcc
CPP      := $(DEVKITARM)/bin/arm-none-eabi-cpp
AS       := $(DEVKITARM)/bin/arm-none-eabi-as
LD       := $(DEVKITARM)/bin/arm-none-eabi-ld
OBJCOPY  := $(DEVKITARM)/bin/arm-none-eabi-objcopy

CC1FLAGS := -mthumb-interwork -Wimplicit -Wparentheses -O2 -fhex-asm
CPPFLAGS := -I $(AGBCC)/include -iquote include -nostdinc -undef
ASFLAGS  := -mcpu=arm7tdmi -mthumb-interwork -I asminclude

#### Files ####

ROM      := {name}.gba
ELF      := $(ROM:.gba=.elf)
MAP      := $(ROM:.gba=.map)
LDSCRIPT := ldscript.txt
SYMBOLS  := symbols.txt
CFILES   := $(wildcard src/*.c)
SFILES   := $(wildcard asm/*.s) $(wildcard data/*.s)
OFILES   := $(SFILES:.s=.o) $(CFILES:.c=.o)

src/agb_flash.o: CC1FLAGS := -O1 -mthumb-interwork
src/agb_flash_1m.o: CC1FLAGS := -O1 -mthumb-interwork
src/agb_flash_mx.o: CC1FLAGS := -O1 -mthumb-interwork

src/libc.o: CC1 := $(CC1_OLD)
src/libc.o: CC1FLAGS := -O2

#### Main Targets ####

compare: $(ROM)
\tsha1sum -c checksum.sha1

clean:
\t$(RM) $(ROM) $(ELF) $(MAP) $(OFILES) src/*.s

#### Recipes ####

$(ELF): $(OFILES) $(LDSCRIPT) $(SYMBOLS)
\t$(LD) -T $(SYMBOLS) -T $(LDSCRIPT) -Map $(MAP) $(OFILES) $(AGBCC)/lib/libgcc.a -o $@

%.gba: %.elf
\t$(OBJCOPY) -O binary --pad-to 0x8000000 $< $@

%.o: %.c
\t$(CPP) $(CPPFLAGS) $< | $(CC1) $(CC1FLAGS) -o $*.s
\t$(AS) $(ASFLAGS) $*.s -o $*.o

%.o: %.s
\t$(AS) $(ASFLAGS) $< -o $@
"
    )?;

    Ok(())
}

fn init_project(gba: &Gba) -> Result<(), Box<dyn std::error::Error>> {
    ldscript_write(gba)?;
    makefile_write(gba)?;
    sha1sum_write(gba)?;
    missing_asm_write(gba)?;

    std::fs::create_dir_all("asm")?;
    std::fs::create_dir_all("src")?;

    Ok(())
}

fn missing_asm_write(gba: &Gba) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("asm")?;

    let segments = &gba.rom.segments;
    if let Some(seg) = segments.first() {
        if seg.address != 0 {
            // missing start
            todo!();
        }
    }

    for window in segments.windows(2) {
        let left = &window[0];
        let right = &window[1];

        if let Some(size) = left.size {
            let hole_start = left.address + size;
            if hole_start < right.address {
                let hole_size = right.address - hole_start;
                let mut file = File::create(format!("asm/rom_{hole_start:06X}.s"))?;
                writeln!(
                    file,
                    ".incbin \"baserom.gba\", 0x{hole_start:06X}, 0x{hole_size:06X}"
                )?;
            }
        }
    }

    if let Some(seg) = segments.last() {
        if let Some(size) = seg.size {
            if seg.address + size < gba.rom.size {
                let hole_start = seg.address + size;
                let mut file = File::create(format!("asm/rom_{hole_start:06X}.s"))?;
                writeln!(file, ".incbin \"baserom.gba\", 0x{hole_start:06X}")?;
            }
        }
    }

    Ok(())
}

fn sha1sum_write(gba: &Gba) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(content) = std::fs::read("baserom.gba") {
        let sha1sum = Sha1::digest(&content);
        let sha1sum = hex::encode(&sha1sum);

        if sha1sum != gba.sha1 {
            eprintln!("baserom.gba doesn't match the sha1 checksum.");
            eprintln!("Expected: {sha1sum}");
            eprintln!("Got:      {}", gba.sha1);
        }
    }

    let mut file = File::create("checksum.sha1")?;
    writeln!(file, "{}  {}.gba", gba.sha1, gba.name)?;

    Ok(())
}

fn build_project() -> Result<(), Box<dyn std::error::Error>> {
    let mut script_command = process::Command::new("make");
    let _ = script_command.exec();
    Ok(())
}

fn update_project(gba: &Gba) -> Result<(), Box<dyn std::error::Error>> {
    clean_project(gba)?;
    ldscript_write(gba)?;
    missing_asm_write(gba)?;

    Ok(())
}

fn remove_split_files(gba: &Gba) -> Result<(), Box<dyn std::error::Error>> {
    let asm_files = fs::read_dir("asm")?;

    for entry in asm_files {
        let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() {
                return Ok(());
            }

            let path = entry.path();
            let Some(extension) = path.extension() else {
                return Ok(());
            };

            if extension != "s" {
                return Ok(());
            }

            let Some(file_stem) = path.file_stem() else {
                return Ok(());
            };

            let Some(file_stem_str) = file_stem.to_str() else {
                return Ok(());
            };

            if !gba
                .rom
                .segments
                .iter()
                .any(|s| s.format == Format::Asm && s.name == file_stem_str)
            {
                let _ = fs::remove_file(path);
            }

            Ok(())
        })();
    }

    Ok(())
}

fn clean_project(gba: &Gba) -> Result<(), Box<dyn std::error::Error>> {
    clean_folder("asm", "o");
    clean_folder("src", "o");
    clean_folder("src", "s");
    remove_split_files(gba)?;

    Ok(())
}

fn clean_folder(name: &str, ext: &str) {
    let asm_files = fs::read_dir(name).unwrap();

    for entry in asm_files {
        let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() {
                return Ok(());
            }

            let path = entry.path();
            let Some(extension) = path.extension() else {
                return Ok(());
            };
            if extension == ext {
                let _ = fs::remove_file(path);
            }

            Ok(())
        })();
    }
}

mod unit_parser {
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<u32, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut s = String::deserialize(deserializer)?;

        let val = if let Some(unit) = s.pop() {
            if let Ok(value) = s.parse::<u32>() {
                match unit {
                    'M' => value * 1_000_000,
                    'K' => value * 1_000,
                    _ => todo!(),
                }
            } else {
                panic!("{s} vs {unit}");
            }
        } else {
            panic!("{s}???");
        };

        Ok(val)
    }
}
