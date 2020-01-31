---
id: adding-more-tasks
title: Adding More Tasks
sidebar_label: Adding More Tasks
---

Now that we've got a single task running, let's expirement a bit more. Very rarely do
you only have one task you need to run. So let's go ahead and add another test to our
project and see how we can make this scale a bit more easy.

First let's create a second python file, with a second test:

***src/app_two.py***

```python
#!/usr/bin/env python3

def decrement(num):
    return num - 1
```

***src/app_two_test.py***

```python
#!/usr/bin/env python3

import unittest

import app_two

class MyModuleTest(unittest.TestCase):
    def test_increment(self):
        assert(app_two.decrement(4) == 3)

if __name__ == '__main__':
    unittest.main()
```

Great so we have this second test file, let's go ahead and do what we know. Create
another task for it. For now let's just copy what we did previously (we'll make
it more reusable right after). So first let's create a new task:

***.dl/tasks/app-test-two.sh***

```shell
python3 ./src/app_two_test.py
```

***.dl/tasks/dl-tasks.yml***

```yaml {15-25}
---
tasks:
  - name: "app-test"
    description: "run the test for the app code."
    location:
      type: "path"
      at: "app-test.sh"
    execution_needs:
      - name: "python"
        version_matcher: ">=3"
    tags:
      - "test"
      - "ci"

  - name: "app-test-two"
    description: "run the test for the app two's code."
    location:
      type: "path"
      at: "app-test-two.sh"
    execution_needs:
      - name: "python"
        version_matcher: ">=3"
    tags:
      - "test"
      - "ci"
```

And now if we run: `dl exec app-test-two` we'll see it works!
*Truth be told that's not very exciting though.* We already kind of knew it would,
and we had to add a whole bunch of extra code! Not to mention there's no easy
way to run both of the tests together. I can only run them one at a time!
That's no fun.

## DRY-ing up the shell scripts ##

The first thing that bugs me (I'm not sure about you) is that I had to create
a second shell script to run essentially the same command! I don't wanna have
to do that. It'd be great if I could just have the same shell script accept an
argument.

Luckily this is possible. So let's change our code a bit to accept an argument from
the user.

- Let's remove `.dl/tasks/app-test-two.sh`, and go back to one file.
- Next let's add change `.dl/tasks/app-test.sh` to the following:

***.dl/tasks/app-test.sh***

```python {2}
# "$1" is a shorthand for the first argument inside a shell script
python3 $1
```

- Finally let's change our task file to reflect our new shell script:

***.dl/tasks/dl-tasks.yml***

```yaml
---
tasks:
  - name: "test-python"
    description: "run the test for a particular file, accepts the file as the first argument"
    location:
      type: "path"
      at: "app-test.sh"
    execution_needs:
      - name: "python"
        version_matcher: ">=3"
    tags:
      - "test"
      - "ci"
```

Great! we're back down to one task. That takes an argument. Let's try it:

<img src="/img/dl-base-arg-task.png" />

There's a problem we've introduced though. If you notice we're no longer declarative. If I
want to say change the folder of `app_test.py` that command will no longer work! It hurts
my ability to make changes later on. I'd have to teach everyone a new command. Let's see
if we can clean that up.

## Introducing Oneof ##

Really what we want is to get back to a declarative cli with one of a series of options.
Where a user can say "I want to run the python-test for application two". So how do we
get there? Well so far we've dealt with tasks that are none as "command" tasks. They
run a particular script. There are actually two other types of tasks `oneof`, and `pipeline`.

We'll cover `oneof` here. Oneof allows us to choose a particular task to run, with certain
arguments based on a name. To make it more clear let's modify our task file to add a oneof type:

```yaml {15-26}
---
tasks:
  - name: "test-python"
    description: "run the test for a particular file, accepts the file as the first argument"
    location:
      type: "path"
      at: "app-test.sh"
    execution_needs:
      - name: "python"
        version_matcher: ">=3"
    tags:
      - "test"
      - "ci"

  - name: "test"
    description: "the top level test command"
    type: "oneof"
    options:
      - name: "app-one"
        task: "test-python"
        args:
          - "./src/app_test.py"
      - name: "app-two"
        task: "test-python"
        args:
          - "./src/app_two_test.py"
```

So let's disect this configuration. We define a task the same as normal, but instead
of having a location/execution needs we have a series of `options`. These options
have a name (the name a user would run), the name of the task to actually run, and
finally a list of arguments to pass to the task. So now we can run something like:

<img src="/img/dl-base-app-declarative-oneof-first.png" />

This is great, but if we do a list command we can still only see only the first
possible option. The one that takes an argument. We probably don't want people
using that, only the declarative option. Luckily dev-loop has support for marking
a task as something that cannot be run directly. It's what known as an internal task.

To do this we simply add: `internal: true` to our `test-python` task so it becomes:

```yaml {3}
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
```

Now when we perform a list command we can see that particular option no longer appears:

<img src="/img/dl-base-app-no-list-show.png" />

And if we tried running it directly we'll see dev-loop will helpfully let the user know
you can't do that anymore:

<img src="/img/dl-base-app-no-run-internal.png" />

Finally if you're wondering "how can I discover what paticular commands are available in a oneof besides looking at config?" you can run list with the argument of the task you want to delve into:

<img src="/img/dl-base-list-oneof.png" />

Next let's discover how to run multiple tasks in a row so you can run all the tests at once.
