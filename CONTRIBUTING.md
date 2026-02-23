# Contributing to NeuraOS

Hey everyone, thanks for wanting to help out with NeuraOS. This project is open source and I would love to get some contributions from you guys. 

Before we get into the details, I want to give a huge shoutout to Darrien Rafael Wijaya. He is the one who came up with the original idea and concept for this whole project. Without his vision, this would not exist.

### How to contribute

If you want to add a feature or fix a bug, here is the basic workflow:

1. Fork the repository to your own GitHub account.
2. Clone it to your computer.
3. Create a new branch for whatever you are working on.
4. Write your code and make sure it works.
5. Push it to your fork and open a pull request.

### Setting up your environment

You will need Rust installed to work on this. Just run the standard build command to make sure everything compiles:

```bash
cargo build
```

If you want to test your changes, just run the project:

```bash
cargo run
```

### Adding new stuff

If you want to add a new app, you can look at the neura-apps crate. You just need to create a new module, implement the App trait from neura-app-framework, and register it in the main file. 

If you want to add a new AI provider, check out the neura-ai-core crate. You can implement the AiProvider trait and add it to the factory.

### Some quick rules

* Try to keep your code clean and readable.
* Make sure your pull requests are focused on one specific thing.
* Test your stuff before submitting.

Thanks again for helping out. Let me know if you have any questions.
