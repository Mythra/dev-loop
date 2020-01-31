---
id: our-first-task
title: Creating our First Task
sidebar_label: Our First Task
---

Once we've got our project setup we need to actually tell it to do something. After all it
wouldn't be very exciting if all we did was show a page of mostly empty sections, now would it?

To execute "things" dev-loop has the concept of tasks. A task can best be described as some
unit of work. That is, it does one single thing, and does it (hopefully) well. We want to keep
tasks as doing one thing, and one thing only since it means they can be reused later on.

For us we're going to create a task that actually builds code. We're going to write some python
code, but don't worry. You don't actually need to know python, and there's nothing python specific
about this. We're just using it as an example so we have something to test.

Go ahead, and create a folder called `src/` and add the following two files:


***src/app.py***

```python
#!/usr/bin/env python3

def increment(num):
	return num + 1
```

***src/app_test.py***

```python
#!/usr/bin/env python3

import unittest

import app

class MyModuleTest(unittest.TestCase):
	def test_increment(self):
		assert(app.increment(4) == 5)

if __name__ == '__main__':
	unittest.main()
```

So we've got these python files, now what? Well now we need to actually need to use them. We've got a script,
and a test script. So let's go ahead, and figure out how to run the test script.

## Setting Up The Basic Config ##

To create a task first we need to create a folder for them to be defined in. We generally like
defining them inside of a folder called: `.dl/tasks/`, but there's no hard requirement to this.
For now though let's go ahead and create it:

```shell
mkdir -p ./.dl/tasks/
```

Next we need to tell dev-loop that's where we want our tasks to be stored. So let's open up: `.dl/config.yml`.
When you do you'll see this:

```yaml
---
{}
```

Let's go ahead, and do replace that with:

```yaml
---
task_locations:
  - type: "path"
    at: ".dl/tasks"
    recurse: true
```

So what does this config do? Well it defines a list of locations to find tasks at. A
location can be one of multiple things. It can be a remote file, a file local on the file system, etc.
So we tell it that it's a "path" on our system. (That's the: `type: path` line).

Next we need to tell it where that actual path is at (That's the: `at: .dl/tasks` line, this is relative
to the root of our project).

Finally we tell it: `recurse: true`, this allows us to create multiple folders under: `.dl/tasks`, and have
them all read.

## Creating The Task ##

Tasks are defined in: `dl-tasks.yml` files. This makes it easy to search for where tasks may be defined.
So let's go ahead, and create a task file that will run our tests:

***.dl/tasks/dl-tasks.yml***

```yaml
---
tasks:
  - name: "app-test"
    description: "run the test for the app code."
    location:
      type: "path"
      at: "app-test.sh"
    execution_needs: []
    tags:
      - "test"
      - "ci"
```

Here we defines a list of `tasks` which has one item. That one item is named "app-test".

There is a single description field, which will show up when we list the tasks on the CLI.

It contains a location of a script to actually run "app-test". We haven't created this yet, but when we do it will be in the `.dl/tasks` folder. This path is relative to the actual task definition.

`execution_needs` we will ignore for now until we get to the next section.

Finally we assign some tags to this task. These also aren't too important for now but will become important in a later step.


Now that we've defined our task, we actually need to implement it. So let's create it:

***.dl/tasks/app-test.sh***

```shell
python3 ./src/app_test.py
```

Wooh we're done right? Unfortunately not! We haven't told dev-loop where to actually run our code. You see dev-loop
needs to know where your code wants to run. Since we don't really care where we run, than we need to tell dev-loop
where we want things to go *by default*. This way dev-loop knows what to do. We'll go more into ***why*** you need
to tell it where to run in the next section.

For now let's just update our config file to the following:

***.dl/config.yml***

```yaml {2-3}
---
default_executor:
  type: "host"

task_locations:
  - type: "path"
    at: ".dl/tasks"
    recurse: true
```

Now let's run our task, and see it's output:

<img src="/img/dl-base-app-test.png" />
