<h1 align="center">p2panda-rs</h1>

<div align="center">
  <strong>All the things a panda needs (for testing)</strong>
</div>

<br />

<div align="center">
  <!-- CI status -->
  <a href="https://github.com/p2panda/p2panda/actions">
    <img src="https://img.shields.io/github/workflow/status/p2panda/p2panda/Build%20and%20test?style=flat-square" alt="CI Status" />
  </a>
  <!-- Crates version -->
  <a href="https://crates.io/crates/p2panda-rs">
    <img src="https://img.shields.io/crates/v/p2panda-rs.svg?style=flat-square" alt="Crates.io version" />
  </a>
  <!-- NPM version -->
  <a href="https://www.npmjs.com/package/p2panda-js">
    <img src="https://img.shields.io/npm/v/p2panda-js?style=flat-square" alt="NPM version" />
  </a>
</div>

<div align="center">
  <h3>
    <a href="https://github.com/p2panda/p2panda">
      Installation
    </a>
    <span> | </span>
    <a href="https://docs.rs/p2panda-rs">
      API
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/handbook#how-to-contribute">
      Contributing
    </a>
  </h3>
</div>

<br />

This library provides tools used for testing the [`p2panda-rs`] and [`p2panda-js`]. It includes:

- a mock node
- a mock client
- methods for generating test data (used in `p2panda-js` tests)
- interactive shell playground

[![asciicast](https://asciinema.org/a/kWYR2h59i2DkqW98vlJOoSQPv.svg)](https://asciinema.org/a/kWYR2h59i2DkqW98vlJOoSQPv)

## Development

You will need the following tools to start development:

- [Rust](https://www.rust-lang.org/learn/get-started)

```bash
# Generate test data json output (from `main.rs`)
cargo run

# Launch the interactive shell
cargo run --bin shell

```

## Test Data

Test data is generated as a json document formatted as summerised below (to see full output, run `cargo run`). Currently in the `p2panda-js` tests we need the data to consist of a single author with a single log which contains 4 entries published with the following message types in this exact order -> 1: CREATE, 2: UPDATE, 3: DELETE and 4: CREATE.

```js
{
  // arbitrary name for identifying author in tests
  "panda": {
    "publicKey": "...",
    "privateKey": "...",
    "logs": [
      {
        "encodedEntries": [
          {
            "author": "...",
            "entryBytes": "...",
            "entryHash": "...",
            "payloadBytes": "...",
            "payloadHash": "...",
            "logId": 1,
            "seqNum": 1
          },
          //...
          {
            "author": "...",
            //...
            "seqNum": 4
          }
        ],
        "decodedMessages": [
          {
            "action": "create",
            "schema": "...",
            "version": 1,
            "fields": {
              "message": {
                "type": "str",
                "value": "..."
              }
            }
          },
          {
            "action": "update",
            //...
          },
          {
            "action": "delete",
            //...
          },
          {
            "action": "create",
            //...
          }
        ],
        "nextEntryArgs": [
          {
            "entryHashBacklink": null,
            "entryHashSkiplink": null,
            "seqNum": 1,
            "logId": 1
          },
          //...
          {
            "entryHashBacklink": "...",
            "entryHashSkiplink": null,
            "seqNum": 5,
            "logId": 1
          }
        ]
      }
    ]
  }
}
```

## Interactive CL playground

The interactive shell is intended as a playground are for exploring p2panda features. It utilizes current (and some future) p2panda features and runs against a mock node (there is no networking involved). 

Available commands:

```
├── author
│   ├── new
│   ├── list
│   ├── set
│   └── whoami
├── create
│   └── chat
├── update
│   └── chat
├── delete
│   └── chat
├── instances
│   ├── list
│   └── find
└── entries
    └── list


Builtins
├── help
├── helptree
├── exit
└── history
```

### Examples

```sh
# create a new author called panda
author new panda

# list all authors
author list

# become a different author (author must already exist)
author set penguin

# check who you are
author whoami

# publish a create message following the 'chat' schema, all trailing arguments make up chat message value
create chat Hello my name is panda!

# publish an update message
update chat 0040a8cec28dea11a4ba299cc1b0f246dfe8a5f71af61281085beff90b8564b9e4d9f07cd6f048399a2b73a8dc499a9b5d647f32223b190832cdeed087b1a074f698 Hello my name is Hungry Panda!

# publish a delete message
delete chat 0040a8cec28dea11a4ba299cc1b0f246dfe8a5f71af61281085beff90b8564b9e4d9f07cd6f048399a2b73a8dc499a9b5d647f32223b190832cdeed087b1a074f698

# list all entries
entries list

# list all instances
instances list

# retrieve a full instance id from a sub string (needed for publishing UPDATE and DELETE messages)
instances find 087b1a074f698

```

## License

GNU Affero General Public License v3.0 [`AGPL-3.0-or-later`](LICENSE)

## Supported by

<img src="https://p2panda.org/images/ngi-logo.png" width="auto" height="80px"><br /><img src="https://p2panda.org/images/eu-flag-logo.png" width="auto" height="80px">

*This project has received funding from the European Union’s Horizon 2020 research and innovation programme within the framework of the NGI-POINTER Project funded under grant agreement No 871528*
