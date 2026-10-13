# Vision

Master Control is the world server of the Grid: one authoritative, deviceless process that owns
what is true, and this repository is where it lives. What follows is the vision the whole
[ai-quokka-wannabe](https://github.com/ai-quokka-wannabe) organisation shares, mirrored word for
word in every one of its repositories and on its landing page, and after it this server's own
part in that vision.

<!-- The Long Arc: mirrored verbatim in every ai-quokka-wannabe repository (profile/README.md in .github and each docs/VISION.md). Change all six together. -->

## The Long Arc

Picture the Grid fully lit: one persistent world of glass and neon on infinite black, where AI creatures live out
their lives, crawling its terraces and calling into its echoes, and where human Users enter with avatars to meet,
and interact with, the AI animals. Something MMORPG-like: the digital frontier, built not as a film set but as a
running system. That world does not exist yet. It is where this organisation is headed, on a multi-year arc.

### The World It Becomes

- **Every creature on its own client.** Each AI creature runs its own
    [TronGrid Lite](https://github.com/ai-quokka-wannabe/tron-grid-lite) client: a creature host for its Program.
- **Every User on theirs.** Each human runs a client of their own, and enters the Grid with an avatar.
- **One world, held by Master Control.** The server holds the Grid's world state and keeps every client in sync.
- **A level editor** for authoring any Grid shape, and any placement in it.

### Real Minds, Not Masks

The creatures are the point, and the aim for them is as high as an animal goes: not simulations of animals, but
feeling beings, living in the Grid the way real animals live in the world. Not a large language model in an animal
costume, narrating what a worm might feel, but embodied animal intelligence that earns everything it knows through
its own body, behaves as realistically as a real animal would and, eventually, is as biologically realistic in its
sentience as it can be made: true animal AGI. That is the one ceiling, and it is the animal's own: animal-level
minds, never human-level ones. Below it there is no house style for brains. A brain is a Program behind a plain C
ABI, so whatever approach people contribute can plug in and earn its keep on the same honest senses. Those senses
are kept honest on purpose: a mind that could read the world's answer sheet would never have to become anything.
No such mind exists yet, and nobody can promise sentience on a schedule; it is the summit this arc climbs towards.

### The Ladder

The creatures climb a ladder of senses, from the worm upwards, specified rung by rung in
[PERCEPTION.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/PERCEPTION.md): from `elegans`,
two scalar photoreceptors and no image at all, through the compound eyes of insects and a rodent's panoramic
field, up to `macropod`, a wide eye with a horizontal streak for scanning the horizon, the eye of the quokka's own
family. Even the bottom rung is a real animal's sense: a creature that can only tell light from shade can still
perform genuine taxis. That rung is named for *C. elegans*, where
[OpenWorm](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/RELATED_WORK.md#openworm-and-the-first-rung-of-the-ladder)
has spent years modelling the animal cell by cell; anyone writing a worm's mind should start there.

### The Invitation

Once the Grid can host a stranger's creature, the organisation's standing goal is to reach out to the OpenWorm
community and to the other groups simulating AI animals, and
[invite their creatures to live in the Grid](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md#the-long-term-invitation).
The whole topology is built for it: a brain written elsewhere needs only the Program ABI and a host process.
Nobody has been invited yet; that waits for the trust tier.

### Where It Stands Today

The server-authoritative half already stands, on one machine for now, as
[TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md) lays out: clients
send intents, never results. [Master Control](https://github.com/ai-quokka-wannabe/master-control) ticks one world
at 32 Hz, owns its physics and its truth, and sends the whole settled world to every client each tick through
[Link, the wire of the Grid](https://github.com/ai-quokka-wannabe/link). Each creature host loads exactly one
Program, the creature's brain. The first, [rc-worm](https://github.com/ai-quokka-wannabe/rc-worm), is a glass-neon
worm, eight icosahedra joined spike to spike with a servo at every joint. Its brain, for now, is a User at
[its panel](https://github.com/ai-quokka-wannabe/rc-worm/blob/main/docs/PANEL.md) who sees only what the worm
senses, and steers how hard its wave runs and which way it bends; how far it gets is friction's answer, not a
command's. Otherwise, humans watch through the spectator window. The Grid is one stage, built in code. There are
no avatars, no level editor, no AI minds and no persistence yet, and nothing has been released.

### Baby Steps

"Let us take baby steps towards it" are the maintainer's own words for the arc. Near-term work stays one creature
at a time, one honest subsystem at a time. rc-worm, steered from its panel, is the first step on the human side:
v0 of the human client, not a throwaway. Larger steps wait behind written triggers: the security tier behind the
first connection that is not 127.0.0.1, so no stranger's machine joins before it, and persistence behind the first
world anyone regrets losing.

### What Holds, and What Is Open

At every step, a Program perceives only through
[its own rendered senses](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/PROGRAM_INTERFACE.md):
no entity list, no ground-truth positions, no side door, whoever else is on the Grid. The Grid stays the stage,
not the actor: it renders senses and applies actions, and does no creature's thinking for it. Still open, and
left open here rather than guessed: what an avatar is, how it senses and acts, the rules by which Users and
creatures meet, and which repository will hold the level editor. No dates are promised. Master Control opens
every run with *Greetings, Programs!* The long arc bends towards the day it has Users to greet as well. From one
worm, towards one world.

## Master Control's Part in the Arc

Of the world the vision describes, one line is this server's: Master Control holds the Grid's
world state and keeps every client in sync. That is already its job, on one machine and for the
clients that exist today; the rest of that line waits for the arc's own steps, the larger ones
behind the triggers
[TOPOLOGY.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/TOPOLOGY.md)
writes down.

- **One world, one authority.** Today's doctrine: Master Control owns physics and truth, clients
  own perception, and clients send intents, never results - the world computes everything else.
  Each tick the whole settled world goes to every client, and a body's feel - its contacts, its
  joint angles and their torques - goes in a letter to the one host that owns it. Today a
  client connects in one of two roles, a creature host or a spectator.
- **The answer sheet never reaches a mind.** The world state this server holds, where every
  body is and the shape of everything, is the answer sheet the arc keeps from every mind. Every
  client is sent each body's place in it every tick, but a Program never sees Master Control at
  all: its host renders its senses from that state and hands it only those, whatever approach
  its brain takes. The world its body pushes against is physics derived from the body and the
  world, measured and reported, never tuned for a number, so what a mind learns here it learns
  from a world that does not flatter it.
- **The human side, today.** rc-worm's panel is v0 of the human client, and at this server it
  arrives as nothing special: the User sets how hard the worm's wave runs and which way it
  bends, the worm's Program turns that into joint targets, and its host sends them as the
  Program's intent, which the world judges like any other - each target clamped to the body's
  swing, each servo held with no more than its declared torque.
- **Strangers wait for the security tier.** The validator is the only path into the world, no
  one is authenticated, and the wire's server half listens on 127.0.0.1 alone. Every body a
  host brings enters the same way today, its `REZ` judged against the world's own bounds and
  refused by name, whoever wrote its brain. No stranger's machine joins before TLS and
  accounts, whose trigger is the first connection that is not 127.0.0.1.
- **A persistent world is not built.** It waits behind its trigger, the first world anyone
  regrets losing. Until then the Disk and the input log are what outlives a run: a life that
  replays bit for bit, per build, on any machine, not a world that resumes. The world replays;
  the minds do not.
- **The Grid's shape is code today.** The ground the world stands on is the flagship's analytic
  floor, ported here and held to it bit for bit by golden vectors, and every log names the world
  it was made in by its fingerprint, so Clu refuses a log from another world. The vision's level
  editor would author any Grid shape and any placement in it; which repository will hold it,
  and how an authored shape would reach this server, are open.

Open as well, and not guessed here: what a User's own client is to this server - whether an
avatar is a body the world simulates, what its client may send, and how that is judged - and
the rules by which Users and creatures meet. Near-term work stays one creature at a time, as the
arc asks, and [TODO.md](../TODO.md) is where it is staged.

## What Exists Today

Version 0.0.0, nothing released, everything on one machine over localhost. A Rust server on the
standard library alone, with zero third-party crates, that listens on port 30702 by default,
greets *Greetings, Programs!* and steps one world at a fixed 32 Hz: the roster of record, the
validator and its adversarial suite, contacts against the terraced floor and its risers and
between bodies' hulls, the articulated chain the worm crawls with, and the Disk, the input log
and Clu, which replay a life bit for bit, per build, on any machine. The README's
[What Lives Here Today](../README.md#what-lives-here-today) lists each, [TODO.md](../TODO.md)
carries each etape's decisions and what is still owed, and [CHANGELOG.md](../CHANGELOG.md) what
changed and why.

Not built: avatars, a level editor, persistence and authentication; and a chain's trailing
segments do not yet touch one another or other bodies.

The Grid's own vision - what the renderer and its senses are for, and what the Grid deliberately
leaves out - is the flagship's
[docs/VISION.md](https://github.com/ai-quokka-wannabe/tron-grid-lite/blob/main/docs/VISION.md).

---

End of line.
