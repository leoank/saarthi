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

## When it does not work

- **No book icon**: no connected node lists `yajna`; `saarthi ctl nodes` shows each node's
  services.
- **"nothing answers" under a node**: the service has no `start` command and is not
  running, or its command failed; the node's log says which.
- **A notebook missing**: it is not in yajna's registry on that machine; run any `yajna`
  command in it. Notebooks under a temporary directory are left out.
