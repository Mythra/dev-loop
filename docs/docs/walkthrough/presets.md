---
id: presets
title: Presets
sidebar_label: Presets
---

So now that we've got all of our tasks running, and all the building blocks we can
build every type of task. However, there's only a way to run one task at a time.
As our task list grows it's going to get harder, and harder to juggle all of the
tasks, and knowing what runs when.

Dev-Loop exposes a solution for these problems with "presets". If you remember
when creating tasks, we tagged our tasks. However we never saw a use for them.
This is exactly where presets come in. A "preset" is just a named series of tags.

So for example, I could tag things: `lint`, and `test`. Then create a preset called
`ci` that runs anything tagged `lint`, and `test`. To do that all I need to do is
modify our configuration:

***.dl/config.yml***

```yaml {12-17}
---
default_executor:
  type: "docker"
  params:
    image: "python:3.8-slim"
    name_prefix: "python-"
  provides:
    - name: "python"
      version: "3.8.0"
    - name: "linux"

presets:
  - name: "ci"
    description: "run all the lint & test tasks"
    tags:
      - "lint"
      - "test"

task_locations:
  - type: "path"
    at: ".dl/tasks"
    recurse: true
```

To run a preset instead of using the: `exec` subcommand you use `run`. So
to run the example we just wrote it would be: `dl run ci`.

Run will automatically use the best concurrency setting for your platform
to run all the tasks as fast as possible.
