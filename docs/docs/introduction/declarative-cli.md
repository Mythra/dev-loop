---
id: declarative-cli
title: Creating a Declarative CLI
sidebar_label: Creating a Declarative CLI
---

When We Say "We'll make your CLI declarative." You may be puzzled as to what we actually mean. It's
not the clearest thing when you have no context. Declarative is often used in various programming libraries
with varying levels of correctness.

When we say "We want to make the CLI declarative" what we mean is we want you to express what you want to have happen as opposed to running a specific tool.

In essence we want to turn:

```shell
GOOS=linux go build -ldflags="-s -w" ./cmd/my-cmd/
```

Into:

```shell
dl exec build my-cmd
```

Here you're telling what you want to have happen. "I want to execute the build task for my-cmd."
Whereas the previous one you're still saying I want to build my-cmd, but there's extra noise.

Noise that can change over time. What if I wanted to change the ldflags argument to include something else? Now
I have to re-teach everyone the command of how to build my application.

Where as with dev-loop because it's an abstraction, the actual underlying command is never exposed. I can change the ld-flags default
for everyone, without having to teach everyone the new command.

When I say this you may have one of two thoughts:

* "Ah it's nice I can make these breaking build changes easier".
* "Great, now I'll never be able to figure out why my build is breaking".

## Notes on Debugging the "Declarative" CLI ##

I understand you're concern with abstracting away the command you run! I know there are certainly concerns the more
abstractions you add in. I'd encourage you to try testing it for yourself, but i'd also like to
mention some of the things we do to lessen this concern:

* All logs that come from a task are tagged with the task name that they came from, and task names are globally unique.
  You can directly go from this task name into a bash script, and read what it's doing.
* By abstracting away the runtime environment we remove many of the cases for "it works on my machine but not on yours"
  meaning this is much more likely to happen only when you're actively changing a task, and you actively have context.
* While we don't modify your code in anyway we do load the helper scripts you write for you, this could create a problem "abstracted" by
  the magic, to combat this by turning up the log level to info you will see the directory we place all of our files, including
  the simple bash script that sources in your helpers and in what order, so you can more easily debug.

We really do suggest you try it yourself though, we've tried really hard to make it as easy as possible so you don't end up
in the Makefile world of "I can't figure out how this task is run". Obviously it won't be perfect for everyone, but the only
way to know is to try it, and file issues for things you run into.
