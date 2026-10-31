# Swarmception

A tool for looking at swarm behaviour: it reads any swarm's `tracking` branch (the events every
agent of a [swarm-core](https://github.com/mornycorp/swarm-core) swarm logs) and shows what each
agent did on [swarm-timeline](https://github.com/mornycorp/swarm-timeline)'s timeline: sessions,
work on issues, sub-agents (gates, `ready` checks) and events, with rows by worker or by ticket,
as a tree or flat, collapsed or not.

```sh
cargo run -- --repo ../swarm-timeline
```

| Crate | What |
|---|---|
| `swarmception-model` | The tracking format's events, parsed |
| `swarmception-domain` | Sessions, work, sub-agents and rows from the events; the timeline's data |
| `swarmception-source` | The tracking branch through git, with deadlines |
| `swarmception-engine` | The session and its typed commands |
| `swarmception-ui` | The egui shell: menus, toolbar, panes |
| `swarmception`, `swarmception-cli` | The app and the headless CLI |

[docs/architecture.md](docs/architecture.md) has the layers. It is built by an agent swarm on
swarm-core: agents start at [AGENTS.md](AGENTS.md); where it stands is [ROADMAP.md](ROADMAP.md).
