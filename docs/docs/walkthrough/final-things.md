---
id: final-things
title: Final Things
sidebar_label: Final Things
---

So now you've got all you need to do right? You can write tasks, you can
define executors. What more do you need to know? Pretty much nothing! These
are the two core building blocks that you can use to model what we believe
is everything you need to do inside your repository.

However, there is one more concept you should know about. As you build your
tasks you may find there's a lot of repitition. That is you may find
you need help making your shell script code more DRY, or running some particular
commands at startup.

Dev-Loop has a concept called helpers. Helpers are just simply just shell scripts
that run before every single task, and are automatically sourced into
every task.

## Defining A Helper ##

Since we didn't need any for our particular task, let's go ahead, and just
create a dummy helper. The first thing we need to do is create a place for
helpers, and tell dev-loop where it is:

- `mkdir -p .dl/helpers/` To create the new helper folder.

- Than we edit the config file:

***.dl/config.yml***

```yaml {19-22}
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

helper_locations:
  - type: "path"
    at: ".dl/helpers"
    recurse: true

task_locations:
  - type: "path"
    at: ".dl/tasks"
    recurse: true
```

Let's create a helper file (which is a shell script file denoted by `.sh` file suffix):

***.dl/helpers/helper.sh***

```shell
addTwoNumbers() {
  local readonly number_one="$1"
  local readonly number_two="$2"

  echo $(( number_one + number_two  ))
}
```

There we go! We've got a function. It's pretty useless, but let's use it in one of our tasks:

***.dl/tasks/app-test.sh***

```shell {1}
addTwoNumbers 1 1
python3 $1
```

Note we didn't have to source in the helper. Helpers are always sourced in regardless of what
task is being run, or where it's being run. So now if we run this task we'll see:

<img src="/img/dl-helpers-one.png" />

## Helpers Depending on Each Other ##

Helpers are not guaranteed to run in any specific order. So if you need dependencies among helpers
you just need to source them in. So for example if I wanted to create helper two, that depended on
helper one:

***.dl/helpers/helper_two.sh***

```shell
source ".dl/helpers/helper.sh"

addTwoNumbers 1 1
```

This will create a dependency between to the two helpers. It is because of this a helper
*may be sourced multiple times*. So all of your helpers that do anything that you don't
want running twice you should guard:

```shell
if [[ "x$HELPER_HAS_BEEN_SOURCED" == "x" ]]; then
  doDestructiveAction
  HELPER_HAS_BEEN_SOURCED="1"
fi
```

This way if it gets sourced again nothing bad will happen. To be clear this is what it looks like
when you run it:

<img src="/img/dl-helpers-depend.png" />

## Fin ##

With that out of the way, that's everything dev-loop has to offer! There's literally nothing more!
We've given you all the base components you need to build anything you might need.

If you have any problems please feel free to file an issue, and good luck building!
