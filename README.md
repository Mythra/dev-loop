# Dev-Loop #

Dev-Loop (also known as "dl") is a tool meant to aid in your dev-loop.
Specifically it aims to fill the replacement of Makefiles, or the standard
`bin/`/`ci/` script directory. The one where we have a series of bash scripts
that are complex, and easy to break cross operating system. Dev-Loop is meant
to make it easy to write _workable_ code that will run the same on every
operating system. While also making it clear to seperate concerns. The four
biggest goals of dev-loop are:

  1. Stability - At the command, config, and implementation levels.
  2. Reproducibility - Everything needs to be reproducible.
  3. Reusability - It should be easy to build small tasks, that can be reused many times.
  4. Expected - We shouldn't surprise a user.

## Overview of the Tool ##

So how does Dev-Loop actually make it easy for you to develop locally?

- Has "Executors". Executors are used to provide a consistent runtime
  environment. However, they also bridge the gap between themselves, and
  the localhost. To make that more clear it allows you to run a task in
  a docker container so you don't have to install everything, but the built
  binary is still available on your local host.

- Formally defining tasks in: `dl-tasks.yml` files. No longer do you have to
  search through bash scripts in order to find what's runnable, and what's
  a helper. Config is much easier to parse in order to figure out exactly
  which scripts are being run and in what order.

- The ability to tag tasks. No longer do you have to have a forever growing
  list of commands in your CI file. Just tag something: `ci`, and tell dev-loop
  to run everything with that tag.

## Installing Dev-Loop ##

To Install Dev-Loop all you need to do is install a single binary. To get the
latest release binary simply go to the [releases page](https://github.com/SecurityInsanity/dev-loop/releases).

You can download the latest binary from github releases. Than all you need to do
is move it into your PATH. If you don't know what that is (or you're looking
for a particular recommendation) you'll just want to move the binary from
your downloads to one of the following event places:

  * On Unix Systems: `/usr/local/bin/` (i.e.: `mv -f ~/Downloads/dl-linux /usr/local/bin/dl`)
  * On Windows Systems: `C:\Windows` (i.e.: `MOVE -Y C:\Users\[User]\Downloads\dl-win.exe C:\Windows\dl.exe`)

## Building Dev-Loop ##

All of the tasks within dev-loop are actually built with dev-loop itself.
While you can certainly bootstrap with an older version of dev-loop, you can
also trigger a build locally. This guide covers both.

If building with a previous version of dev-loop you do not need to install
anything besides docker.

### Building with a previous version of dev-loop ###

The way we recommend getting started is using a previous version of dev-loop.
Follow the "Installing Dev-Loop" section to install a previous version of
dev-loop.

From there you can start building code with:

```shell
$ dl exec build dl
```

This will put a binary in: `./target/dl`

To build a binary ready for release use:

```shell
$ dl exec build dl-release
```

This will put a binary in: `./target/dl-release`

### Building from Scratch ###

In order to build from scratch please ensure you have the latest version of
Rust installed. Please use [rustup](https://rustup.rs/) to install rust.
Since we will need to add support for another "target" in order to build a
fully static binary.

Once you've installed the latest rustup. Please follow: [these docs](https://doc.rust-lang.org/edition-guide/rust-2018/platform-and-target-support/musl-support-for-fully-static-binaries.html)
to add the musl toolchain. Which will help us get a fully static binary.

Once you do that you can run:

```
cargo build --release --target x86_64-unknown-linux-musl
```

This will build a binary, and stick it at: `./target/x86_64-unknown-linux-musl/release/dev-loop`