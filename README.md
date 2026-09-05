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

Checks `baserom.gba` has the correct checksum. \
Creates the `src/` folder (if it doesn't exist). \
Calls `linker`. \
Calls `split`.

### linker

Generate the linker script.

### split

Extract the untouched parts of the ROM.

### update

The command to use to regenerate the files when `project.yml` changes, instead of calling `clean`, `linker`, and `split` manually.

### build

Build the project based on the commands in the `build` object.

### clean

Removes build artifacts (e.g. `.o` files).
