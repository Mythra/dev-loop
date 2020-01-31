---
id: preset-conf
title: PresetConf
sidebar_label: PresetConf
---

A preset provides a way to run multiple tasks at once. It is also useful when you want too
not have to maintain a huge list of tasks to run in a specific scenario (for example ci).
You can just tag everything you want to run at ci time: `ci`, and than create a preset that runs everything
tagged `ci`. This way there's not one single file with a high rate of change.

- `name`: String [REQUIRED]

The name of this preset. This will be how you actually run this preset, so it's name should be meaningful.

- `description`: String [OPTIONAL]

A description of this preset to help describe what this preset does to a user.

- `tags`: List[String] [REQUIRED]

A list of tags to run when this preset is invoked.
