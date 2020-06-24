---
id: task-conf
title: TaskConf
sidebar_label: TaskConf
---

Describes a "task", part of the graph that dev-loop builds to know how to turn a command into something runnable.

- `name`: String [REQUIRED]

The name of the task. This needs to be unique across the entire project.

- `type`: String [OPTIONAL]

The type of task this is. Currently there are three supported options `command`, `oneof`, `pipeline`, and `parallel-pipeline`.
If you do not specify a type `command` will be assumed.

- `description`: String [OPTIONAL]

The description of this task. This will be used when showing the task inside of a list command.

- `location`: <a href="/docs/schemas/location-conf" class="internal-link">LocationConf</a> [REQUIRED for "command" type tasks] [IGNORED for "oneof"/"pipeline" tasks]

The location of the shell script to run when this task is a "command" type (the location being relative to wherever this task is defined).
If specified on a oneof/pipeline task it will have no effect.

- `execution_needs`: List[<a href="/docs/schemas/needs-requirement" class="internal-link">NeedsRequirement</a>] [OPTIONAL]

A list of things this task needs. This is how you can select a particular executor. If you've specified a `custom_executor` these will have no effect.
If you don't specify these, or a `custom_executor` the Default Executor will be selected if it exists.

- `custom_executor`: <a href="/docs/schemas/executor-conf" class="internal-link">ExecutorConf</a> [OPTIONAL]

Represents a custom executor for this one task. In general we don't recommend setting this, and just defining it inside an executors configuration file.
However there may be specific cases where you want to make it clear this task has needs for a very special executor.
A `custom_executor` will always be selected if specified, and can still be reused within a pipeline.

- `steps`: List[<a href="/docs/schemas/pipeline-step" class="internal-link">PipelineStep</a>] [REQUIRED for "pipeline"/"parallel-pipeline" type tasks] [IGNORED for "command"/"oneof" tasks]

An ordered list of steps to run when running a pipeline.
If specified on a command/oneof task it will have no effect.

- `options`: List[<a href="/docs/schemas/oneof-option" class="internal-link">OneofOption</a>] [REQUIRED for "oneof" type tasks] [IGNORED for "command"/"pipeline" tasks]

A list of options to potentially choose from when the task is a oneof type.
If specified on a command/pipeline task it will have no effect.

- `tags`: List[String] [OPTIONAL]

A list of tags to apply to this task. Tags can be selected by presets in order to run multiple things at a time.

- `internal`: Bool [OPTIONAL]

Whether or not this task is "internal". If a task is internal it will not be shown on any list command, and
cannot be run directly (it must be invoked through a `oneof`/`pipeline`/`parallel-pipeline`).
All internal tasks must be used at least once, or an error will occur because it would be impossible
for that task to do anything.
