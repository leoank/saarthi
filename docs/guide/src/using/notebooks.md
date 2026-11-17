# Notebooks

The app shows the [yajna](https://github.com/leoank/yajna) notebooks on your machines: the
notebook's site and its files viewer, through the hub, with nothing opened on the machine
or the phone.

## What a machine needs

- yajna 0.31.1 or later, whose `yajna serve --socket` answers for every notebook in your
  yajna registry. A notebook joins the registry the first time you run `yajna` in it.
- The node offering it as the service named `yajna`, in its services file
  ([Local services](../setup/node.md#local-services)), with a `start` command so the node
  starts it on the first request.
- A hub and node at 0.9.0 or later.

## In the app

The book icon on the sessions screen appears once a node offers `yajna`. It lists each such
node's notebooks, asking the node for them each time the list opens; a node whose notebook
server does not answer shows why, without holding up the others.

Tapping a notebook opens its site: the project pages, the log and the tasks. The folder
icon opens its files viewer, which follows open files as they grow, as it does in a browser.
Back goes back through the pages you visited, then to the list. Links that leave the
notebook open in your browser.

## Starting a project

*New project* on the notebooks screen starts one in a lab notebook (a project-shaped
notebook is a single project already). Pick the notebook, the kind of project (general,
software, wet lab, machine learning or hardware, each with its own yajna command), write a
sentence or two about it, and start the interview. The node starts a session in the
notebook on `/yajna:project` (or the kind's command) with your brief, and the app opens it.

Claude reads the notebook first, then interviews you one topic at a time. Its questions
come as question cards, as in any session: pick an option, or answer in your own words. Nothing is written until the interview has settled the project. The notebook
must be under one of the node's spawn roots, and the yajna plugin must be installed for
the `claude` the node starts.

## When it does not work

- **No book icon**: no connected node lists `yajna`; `saarthi ctl nodes` shows each node's
  services.
- **"nothing answers" under a node**: the service has no `start` command and is not
  running, or its command failed; the node's log says which.
- **A notebook missing**: it is not in yajna's registry on that machine; run any `yajna`
  command in it. Notebooks under a temporary directory are left out.
