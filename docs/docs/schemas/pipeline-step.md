---
id: pipeline-step
title: PipelineStep
sidebar_label: PipelineStep
---

Describes a particular step within a pipeline.

- `name`: String [REQUIRED]

The name of this step in the pipeline. This isn't displayed anywhere right now.
However, we keep it here because we definetely want to show it some day.

- `description`: String [OPTIONAL]

The description of this particular step in the pipeline. This isn't displayed anywhere right now.
However, we keep it here because we definetely want to show it some day.

- `task`: String [REQUIRED]

The name of the task to run when this pipeline is invoked.

- `args`: List[String] [OPTIONAL]

A list of arguments to pass to the actual task.
