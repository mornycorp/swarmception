# Swarmception

A tool for looking at swarm behaviour: it reads any swarm's `tracking` branch (the events every
agent of a [swarm-core](https://github.com/mornycorp/swarm-core) swarm logs) and its `main`, and
shows what happened to its tickets on [swarm-timeline](https://github.com/mornycorp/swarm-timeline)'s
timeline: every merge onto `main`, every ticket's merge, and each ticket from its claim
through its work to its merge, packed onto shared rows (a ticket on the first row it doesn't
overlap), flat or by worker, collapsed or not.

```sh
cargo run -- --repo ../swarm-timeline
```

| Crate | What |
|---|---|
| `swarmception-model` | The tracking format's events, parsed |
| `swarmception-domain` | Each ticket's life and the rows from the events; the timeline's data |
| `swarmception-source` | The tracking branch and main's merges through git, with deadlines |
| `swarmception-engine` | The session and its typed commands |
| `swarmception-ui` | The egui shell: menus, toolbar, panes |
| `swarmception`, `swarmception-cli` | The app and the headless CLI |

[docs/architecture.md](docs/architecture.md) has the layers. It is built by an agent swarm on
swarm-core: agents start at [AGENTS.md](AGENTS.md); where it stands is [ROADMAP.md](ROADMAP.md).
