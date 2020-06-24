---
id: getting-started
title: Getting Started
sidebar_label: Getting Started
---

The first question you're probably asking is: "what is dev-loop, and why the heck do I care?"
Dev-Loop is in the shortest possible description: "a localized task runner". It can be thought
of as a replacement for a `bin/`, or `ci/` script directory.

* Need to run a build? **Use Dev-Loop.**
* Need to run tests? **Use Dev-Loop.**
* Need to run everything that will run on CI? **Use Dev-Loop.**
* Need to run a one-off task locally? **Use Dev-Loop.**

Dev-Loop is just an abstraction though. It doesn't actually build your code, run your tests,
or run a one off task. Instead Dev-Loop is meant to be an abstraction point for all
these tasks. What do you get out of this abstraction point though? Abstraction for no reason
can be harmful afterall.

NOTE: If you just want to get started using dev-loop we recommend walking through the <a href="/docs/walkthrough/installing" class="internal-link">walkthrough</a>.

NOTE: If you've used dev-loop before, and are just looking for the fields that can be configured, and what they do you probably want the <a href="/docs/schemas/provide-conf" class="internal-link">Schema Documentation</a>.

## What Does Dev-Loop Give Me? ##

There are two real things (we as maintainers) see dev-loop providing here:

- *The Ability to Abstract Away the Runtime Environment*
- *The Ability to Make Local-Dev Declarative*

These two things we'll explain in detail below, but by doing these two things we
can help make your local dev experience:

* ***Stable*** - Even as you change languages/test frameworks/cli options you can keep the same commands in local development that are declartive.
* ***Reproducible*** - Regardless of what OS you're on you will have the same experience as everyone else.
* ***Reusable*** - No longer should you need to copy and paste code around or repeat certain tasks again.

We belive these traits help create a ***sustainable*** CLI for local development.

One that will evolve with your codebase, and teams without creating much friction. Not
to mention it can be gradually adopted. We realize not every team can rewrite their
entire local development experience in one go usually. It takes a lot of effort, and
the tool should help, not hinder you.
