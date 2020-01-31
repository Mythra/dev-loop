---
id: starting
title: Creating a Project to Test Dev-Loop
sidebar_label: Set Up The Walkthrough
---

The first thing you need when starting with dev-loop is a project! A project is almost always the same
as the root folder of a git repository, but it doesn't need to be. In this walkthrough we'll be working
in a project we create ourselves.

To do this let's create a project, let's open our terminal and type the following:

```shell
# Create the following directories at the same time:
#   `dev-loop-walkthrough/`
#   `dev-loop-walkthrough/.dl/`
mkdir -p dev-loop-walkthrough/.dl/
cd dev-loop-walkthrough
echo -e "---\n{}" > .dl/config.yml
```

This creates a "Bare Bones" project setup for dev-loop. A configuration that defines nothing!
If you're to type: `dl` into your terminal now, and hit enter. You'll see the default help
page which may look something like this:

<img src="/img/dl-base-no-config.png" />

If you see a screen that looks like this congratulations! You've successfully done all you need
to do to setup dev-loop!
