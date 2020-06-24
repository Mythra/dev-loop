---
id: pipelines
title: Pipelining Tasks
sidebar_label: Pipelining
---

As mentioned in the previous section we have different types of tasks. You've been introduced to two.
The default "command", and "oneof". Next it's time to introduce the final type of task. A "Pipeline".

A Pipeline runs a series of tasks in a guaranteed order, and much like oneof's they accept a series
of arguments to pass to the underlying tasks so they remain declarative. A pipeline does not parallelize
any of the work it's given, merely is a way of running tasks one after another (in a pipeline)!

If you want a series of tasks that always run in parallel, switch from: `pipeline`, to `parallel-pipeline`,
all the configuration is the same between the two.

This can be useful for doing multi staged docker builds, or setting up something for another task,
or in our case running multiple tests so you don't have to run one at a time! Much like we did for
oneof we're going to introduce a new task to our task file:

***.dl/tasks/dl-tasks.yml***

```yaml {16-27,33-34}
---
tasks:
  - name: "test-python"
    description: "run the test for a particular file, accepts the file as the first argument"
    internal: true
    location:
      type: "path"
      at: "app-test.sh"
    execution_needs:
      - name: "python"
        version_matcher: ">=3"
    tags:
      - "test"
      - "ci"

  - name: "test-pipeline"
    internal: true
    type: "pipeline"
    steps:
      - name: "test-app-one"
        task: "test-python"
        args:
          - "./src/app_test.py"
      - name: "test-app-two"
        task: "test-python"
        args:
          - "./src/app_two_test.py"

  - name: "test"
    description: "the top level test command"
    type: "oneof"
    options:
      - name: "all"
        task: "test-pipeline"
      - name: "app-one"
        task: "test-python"
        args:
          - "./src/app_test.py"
      - name: "app-two"
        task: "test-python"
        args:
          - "./src/app_two_test.py"
```

Although there's quite a few new lines, there's not really *too much* that's actually new
here. The pipeline looks very similar to oneof. Except instead of having: `options`, it has
`steps`. Steps look like options (although they're techincally seperate types). Finally
we tell our oneof task to run that pipeline when specified with the all option. Let's
test it out:

<img src="/img/dl-base-pipeline.png" />

There we go, we can now see that both tasks were executed when we specified all. We've
now got enough to actually fully setup a series of tasks! These building blocks are then
used to piece together, and build all the tasks you need to run locally.
