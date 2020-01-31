---
id: oneof-option
title: OneofOption
sidebar_label: OneofOption
---

Describes a particular option within a series of `oneof` options.

- `name`: String [REQUIRED]

The name of this particular option. This is how a user will invoke the option.

- `args`: List[String] [OPTIONAL]

The list of arguments to pass to the underlying task when running it.

- `description`: String [OPTIONAL]

The description of this particular option. Display when you list this particular option.

- `task`: String [REQUIRED]

The name of the task to run when this option is invoked.

- `tags`: List[String] [OPTIONAL]

A list of tags to apply to this particular option.
