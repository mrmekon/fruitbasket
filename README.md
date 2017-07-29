# fruitbasket - Framework for running Rust programs in a Mac 'app bundle' environment.

[![Build Status](https://travis-ci.org/mrmekon/fruitbasket.svg?branch=master)](https://travis-ci.org/mrmekon/fruitbasket)
[![Crates.io Version](https://img.shields.io/crates/v/fruitbasket.svg)](https://crates.io/crates/fruitbasket)

fruitbasket provides two different (but related) services for helping you run your
Rust binaries as native AppKit/Cocoa applications on Mac OS X:

* App lifecycle and environment API - fruitbasket provides an API to initialize the
  AppKit application environment (NSApplication), to pump the main application loop
  and dispatch Apple events in a non-blocking way, to terminate the application, to
  access resources in the app bundle, and various other tasks frequently needed by
  Mac applications.

* Self-bundling app 'trampoline' - fruitbasket provides a 'trampoline' to
  automatically bundle a standalone binary as a Mac application in a `.app` bundle
  at runtime.  This allows access to features that require running from a bundle (
  such as XPC services), self-installing into the Applications folder, registering
  your app with the system as a document type or URL handler, and various other
  features that are only available to bundled apps with unique identifiers.
  Self-bundling and relaunching itself (the "trampoline" behavior) allows your app
  to get the features of app bundles, but still be launched in the standard Rust
  ways (such as `cargo run`).

The primary goal of fruitbasket is to make it reasonably easy to develop native
Mac GUI applications with the standard Apple AppKit/Cocoa/Foundation frameworks
in pure Rust by pushing all of the Apple and Objective-C runtime logic into
dedicated libraries, isolating the logic of a Rust binary application from the
unsafe platform code.  As the ecosystem of Mac libraries for Rust grows, you
should be able to mix-and-match the libraries your application needs, pump the
event loop with fruitbasket, and never worry about Objective-C in your application.

See the `examples/` dir for demo usage.

## Documentation

[API documentation](https://mrmekon.github.io/fruitbasket/fruitbasket/)
