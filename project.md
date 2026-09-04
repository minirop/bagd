# project.yml syntax

## Fields

- name: The name of the compiled ROM.
- sha1: Checksum of baserom.gba.
- entry: Symbol at address 0x08000000.
- rom: Definition of the ROM.
- ewram (optional and probably useless)
- iwram (optional and probably useless)
- build

## Rom

- address: Start address of the ROM (should be 0x08000000).
- size: Size of the ROM with a unit (e.g. `8M` for a 8MB ROM).
- segments: List of decompiled/disassembled segments of the ROM.

## Segments

- name: Name of the file containing the code.
- format: Can be `c` for `src/$name.c` or `asm` for `asm/$name.s`.
- address: Starting address of this file, relative to the ROM's address.
- size: Size of the compiled/assembled, optional if there are no gaps between this segment and the next.
- section: Section to use (can be `text` or `rodata`), if omitted will be `text`.

## ewram

Fields:
- address: Address of this memory type.
- size: Size with unit (e.g. `32K` for `32KB`)
- symbols: No need for it really, just use `-T symbols.txt` at compile time.

## iwram

Same as `ewram`.

## build

- files: Contains a list of arrays, whose content are a list of files (glob works).
- constants: Defines a list of variables that can be used in other variables or in commands.
- commands: Commands to execute to "build" the project.

### files

Name is the variable name, value is an array of files (glob accepted).

### constants

Name is the variable name, value is a string.

### commands

Can either be a string containing the command to execute or an object containing the folder to look into, the file extension to grab, and a list of commands to execute on all the files found.

## Variables

Inside the content of a constant or in a command, you can use the syntax `$(XXX)` to use a list of files, a constant, or an environment variable.

### Special variables

- `$(ROM)` will contain the field `name` of the project.
- `$(NAME)`, inside an "exec" command, will contain the file path and name without the extension.

## Exemple

```yaml
name: my-rom
sha1: da908c7be42015d8f6f19b911e16f3bc581672e5
entry: Init
rom:
  address: 0x08000000
  size: 8M
  segments:
    - name: crt0
      format: asm
      address: 0x000000
      size: 0x134
build:
    files:
        # $(ofiles) will contains all ".o" files from folders "src/" and "asm/"
        ofiles:
            - src/*.o
            - asm/*.o
    constants:
        as: $(DEVKITARM)/bin/arm-none-eabi-as
        asflags: -mcpu=arm7tdmi -mthumb-interwork -I asminclude
    commands:
        # this assembles all ".s" files in "asm/"
        - folder: asm
          extension: s
          exec:
            - $(AS) $(ASFLAGS) $(NAME).s -o $(NAME).o
        # this command deletes the generated ROM file
        - rm $(ROM).gba
```

Full example at the [Lego Island Decomp](https://github.com/minirop/lego-island-gba/blob/main/project.yml).
