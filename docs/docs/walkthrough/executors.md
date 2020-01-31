---
id: executors
title: Executors
sidebar_label: Executors
---

So now that we've got a task running, let's focus on the question we asked ourselves at the
end of setting it up? Why do we have to tell dev-loop ***where*** something should be run,
and what was that: `default_executor` thing we had to put in our config?

## So What is an Executor? ##

An executor in dev-loop is very shortly defined as something that is capable of providing
a new runtime environment. It may be a Docker Container, maybe it's the Host machine, or
maybe it's even a different machine entirely. What that is doesn't really matter to much.
The point is a "Runtime Environment" is responsible for providing an environment for us
to run code, and then sync the output of back to the host machine. So if you run a docker
container we automatically want to sync back to the host.

In essence an Executor ***should be completely invisible.*** A script should never have to
care whether it's in a remote host, docker container, or running on the host system. This
helps prevent *noise* that tends to pop up when many other tools try to do the right thing.

An Executor is also ***ephemeral.*** It is "stood up"/"torn down" with every time I run
a dev-loop command (to be clear it ***can***, and ***should*** be reused during say a pipeline
of tasks all running to fulfill one command). This means any extra state it has is blown away.
This helps us keep our builds hermetic.

### The "Host" Executor ###

The Host Executor was what we defined in the previous section:

```yaml
default_executor:
  type: "host"
```

The Host Executor as it's name implies runs directly on the host system. It doesn't
do anything different than running a command as you normally would in your terminal.

This is useful when you need to run say a docker command without doing docker in docker,
or if you just want to easily wrap another tool without changing any behavior. There's not
too much to say here. Since it's fairly simple.

### The "Docker" Executor ###

The Docker Executor runs your code in as it describes a docker container. This one
takes a lot of options, but most of the time you'll only have to configure the two
required fields.

To configure fields you pass in: `params:` which contains a map of strings. This means
every value on both sides should be strings. So if you're ever typing in just like a number
make sure to quote it so it gets properly interpreted.

The two required params you need to pass in are: `image` which is the image you'd like your
container to use, and `name_prefix` which defines a prefix for the container name. A full list
of params can be found <a href="/docs/schemas/executor-conf" class="internal-link">here</a>.

NOTE: by default the docker container will mount your projects directory as well as `$TMPDIR`.
This way any file you access inside of your project will automatically be written outside
of the container. If you need extra locations to be saved you can configure them.

## Choosing an Executor ##

So you've got multiple executors, say multiple docker containers you want to choose from. However,
if you notice none of them have names/ids. So how are you supposed to choose which executor to use?

This is where a second argument like `params:` comes in. This argument is called: `provides`. In it
you'd describe what an executor provides. So for example if you wanted a base linux container:

```yaml {5-8}
  - type: "docker"
    params:
      image: "ubuntu:18.04"
      name_prefix: "linux-container-"
    provides:
      - name: "bash"
        version: "4"
      - name: "linux"
```

Here we're saying it provides two things. This is how you end up "selecting" an executor.

### Why do we use Provides as Opposed to Names? ###

The core benefit of using `provides` as opposed to an ID, or name comes through when attempting
to reuse a particular executor. As new executors, and tasks are introduced over time it can be hard to juggle
perfect reusing of executors.

Let's say for example you had one container providing the above, and started inserting it by name into
all of your tasks. Than a little bit later on you introduced another container `linux-container-2-` it introduced
an extra tool for the one task that needed that extra tool. All of a sudden you've created an un-reusable executor.
All the tasks that still reference the old one will not use this new container, even if they're compatible.

By describing what an image provides, and than having tasks describe what they need. You get the most
likely chance to reuse a container when possible which helps speed up execution. Plus it's still
100% possible to use a custom executor for just one task.

## Applying Executors to the Walkthrough ##

Now that we know there are executors that provide different runtime environments. Let's change
the task we set up in the previous step to run inside of docker. We'll do this without changing
any of the code in the shell script, or python. Instead all we're going to do is touch the configuration for dev-loop.

We're going to run in docker so all someone has to install is Docker, and they don't have to install
the correct version of python (nor upgrade python as I upgrade it in the project). This makes the end-user
experience much better over all.

So first we're going to change our task to describe what it needs in order to run. This way it's easier
to tell dev-loop what is actually happening. So let's go ahead, and add the following to our task definition so it looks like:

***.dl/tasks/dl-tasks.yml***

```yaml {8-10}
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
```

The following lines we've added correspond to telling dev-loop that we're looking for an environment
that says it provides python >= version 3. This `version_matcher` argument actually is a semantic
versioning string so it *should* support anything your favorite package manager does for properly
choosing a version of a package.

Now we actually need to teach dev-loop about a docker container that provides python. For now
we'll just keep using the `default_executor` (for larger projects you can define `executor_locations`
which provides a series of locations to search for: `dl-executors.yml` files, *the same way as it works for tasks!*,
you can even provide a custom executor for one specific task through `custom_executor`).

***.dl/config.yml***

```yaml {2-10}
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

task_locations:
  - type: "path"
    at: ".dl/tasks"
    recurse: true
```

This configuration is much bigger, than just the host executor because dev-loop needs a lot
more info on how you want your code run. However with this change, we can now run the exact
same command, and be greeted with the same output (note you may see it take a second, this
is the downloading of the docker container!):

<img src="/img/dl-base-docker-test.png" />

That's it! It may not look very different but, that entire task is running inside a docker
container. Each time you run that task it spins up a docker container,
and spins it down at the end. No longer do you have to manage the container lifecycle, nor
does a user even really need to know how to use docker!
