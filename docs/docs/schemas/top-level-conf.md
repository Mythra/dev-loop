---
id: top-level-conf
title: TopLevelConf
sidebar_label: TopLevelConf
---

The Top Level Configuration is the configuration that sits inside of `.dl/config.yml`.
All of the fields here are optional so you can only opt into the parts of the builds that you need.

- `default_executor`: <a href="/docs/schemas/executor-conf" class="internal-link">ExecutorConf</a> [OPTIONAL]

Define a default executor to use when no other executor has been specified by a particular task. This can help
reduce the amount of configuration you have to write when writing a bunch of new tasks that all have a sane default.

- `ensure_directories`: List[String] [OPTIONAL]

A list of directories to create before running any tasks. This can be useful for cache directories, since
docker requires that a directory exists before it can be mounted in.

- `executor_locations`: List[<a href="/docs/schemas/location-conf" class="internal-link">LocationConf</a>] [OPTIONAL]

Defines a list of directories to look for: `dl-executors.yml` files. These `dl-executors.yml` files are typed as <a href="/docs/schemas/executor-conf-file" class="internal-link">ExecutorConfFile</a>.

- `helper_locations`: List[<a href="/docs/schemas/location-conf" class="internal-link">LocationConf</a>] [OPTIONAL]

A list of locations to look for helpers for. Helpers are identified by having a: `.sh` suffix. They should be shell scripts.

- `presets`: List[<a href="/docs/schemas/preset-conf" class="internal-link">PresetConf</a>] [OPTIONAL]

A list of of presets that can end up being run based on a series of tasks.

- `task_locations`: List[<a href="/docs/schemas/location-conf" class="internal-link">LocationConf</a>] [OPTIONAL]

A list of locations to search for `dl-tasks.yml`. These files have the type of <a href="/docs/schemas/task-conf-file" class="internal-link">TaskConfFile</a>.

As a side note the following environment variables are supported with Dev-Loop:

- `TMPDIR`: String [OPTIONAL]

Used for controlling the temporary directory to mount/use in containers. OSX sets this automatically. If not specified `/tmp/` will be used.

- `DL_WORKER_COUNT`: Unsigned Integer [OPTIONAL]

Used to limit/control the number of workers that dev-loop will use in parallel scenarios.

- `NO_COLOR`: Unset/Set [OPTIONAL]

When no color is set to any value, there will be no color, or fancy text printed to the terminal.

- `DL_FORCE_COLOR`: Boolean [OPTIONAL]

A boolean to flip forcing color of dev-loop. Overrides the `NO_COLOR` setting. Can be useful when you want other tools to not output color that read from `NO_COLOR`, but allow dev-loop to still output color.

- `DL_FORCE_STDOUT_COLOR`: Boolean [OPTIONAL]

A boolean to force coloring output for things printed to STDOUT. Can be useful when dev-loop doesn't detect your terminal as something needing color.

- `DL_FORCE_STDERR_COLOR`: Boolean [OPTIONAL]

A boolean to force coloring output for things printed to STDERR. Can be useful when dev-loop doesn't detect your terminal as something needing color.