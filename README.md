# BAGd

A tool to handle decompilation projects for the GBA.

## Project structure

You just need a project file named `project.yml` and the ROM named `baserom.gba`.

## Commands

```sh
$ bagd <command>
```

### init

The command to use to initialise a project.

Creates `src/` and `asm/` folders (if they don't exist). \
Creates the file `checksum.sha1`. \
Calls some of the commands below.

### linker

Generate the linker script.

### makefile

Generate the Makefile.

### split

Extract the untouched parts of the ROM.

### update

The command to use to regenerate the files when `project.yml` changes, instead of calling `clean`, `linker` and `split` manually.

### build

Calls `make`.

### clean

Removes build artifacts (e.g. `.o` files).

## Notes

For now, it generates a `Makefile` and calls `make`, this might change in the future.
