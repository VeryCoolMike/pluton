# hello!

This is pluton, a federated social network, similar to Matrix. I made it because I just thought it would be cool to, but it is aimed to be as similar to Discord as possible, while being OSS, and having premium features for free. It's federated since I'm too poor to buy proper servers so I'm running everything on a minipc.

## you should NOT use this, YET

Pluton is in a **VERY** early stage, even though I've been working on it for so long. So far we have:

- [x] Pluton server able to connect clients
- [x] Basic messaging
- [x] Channels
- [x] User profiles
- [x] Pluton server database (able to store messages, channels, etc...)
- [x] Pluton home registration
- [x] Pluton home user fetching
- [ ] DMs
- [ ] E2E encryption
- [ ] VC
- [ ] File uploads
- [ ] Pluton desktop
- [ ] Pluton mobile

and all the hard things are the ones that haven't been done yet.

## how to build and use, if you want to for some reason
Oh the beauty of Rust. I don't know how to setup a monorepo so I just put a bunch of rust projects into a folder and called it a day. Run 

```
cargo run
```

in each of the folders to run the program. For pluton-cli, you will probably want to create an account first. For that, run

```
cargo run -- --create_account [USERNAME] [PASSWORD] [HOME SERVER ADDRESS]
```

> NOTE: On Linux, this should create your account in ~/.config/pluton. If you're on some other operating system, install Linux.

replace [USERNAME] with your username, [PASSWORD] with your password, and [HOME SERVER ADDRESS] with your home server address. You can make a home server by going to pluton-home, and running it and then using http://localhost:6768. If you want to create a server then just run:

```
cargo run -- --create_server
```

wow, so easy. Just run cargo run after the server has been made to actually run it. If you want to customise the server then just run

```
cargo run -- --configure_server
```
