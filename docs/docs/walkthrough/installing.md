---
id: installing
title: Installing Dev-Loop
sidebar_label: Installing Dev-Loop
---

Installing Dev-Loop is the first step of actually using dev-loop. For this the essential
steps are just downloading the binary, and putting it in a specific location. We've placed
the instructions below for each platform:

## Installing on Linux ##

To Install on Linux perform the following steps:

- Go to <a href="https://github.com/SecurityInsanity/dev-loop/releases" class="internal-link">to the release page</a> and download the latest: `dl-linux` file.
- Move that downloaded file inside of your `$PATH`, if you're not sure what this is we recommend `/usr/local/bin/` So for example: `sudo mv ~/Downloads/dl-linux /usr/local/bin/dl`.
- Rename the file so it is called: `dl` instead of `dl-linux`. (If you copied the `sudo mv ...` line above this was done for you).
- Done! You can now run: `dl` in your terminal.

If you want to use the Docker Based Executor (Which you almost assuredly want!) You'll want to ensure docker is installed. We support every version from 17.06 and above. To do this please follow the <a href="https://docs.docker.com/install/" class="internal-link">docker install instructions</a>.

## Installing on OSX ##

To Install on OSX perform the following steps:

- Go to <a href="https://github.com/SecurityInsanity/dev-loop/releases" class="internal-link">to the release page</a> and download the latest: `dl-osx` file.
- Move that downloaded file inside of your `$PATH`, if you're not sure what this is we recommend `/usr/local/bin/` So for example: `sudo mv ~/Downloads/dl-osx /usr/local/bin/dl`
- Rename the file so it is called: `dl` instead of `dl-osx`. (If you copied the `sudo mv ...` line above this was done for you).
- Done! You can now run: `dl` in your terminal.

If you want to use the Docker Based Executor (Which you almost assuredly want!) You'll want to do the following:

- Install Docker - To do this please follow the <a href="https://docs.docker.com/install/" class="internal-link">docker install instructions</a>.
- Add your `$TMPDIR` to the list of paths that can be shared (this in the docker settings). If you can't add the `$TMPDIR` just walk the file path up. If you're not sure what this is, you can just add `/var/folders/`.

## Installing on Windows ##

Unfortunately Dev-Loop doesn't quite work on Windows quite yet! We're working on it, but in the meantime we recommend using WSL, and following the linux install instructions.
