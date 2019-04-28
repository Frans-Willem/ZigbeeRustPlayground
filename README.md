# Zigbee Rust Playground

This is an attempt to implement a full Zigbee coordinator, using a USB device running the network stack of the Contiki OS.

## Code reviews ?
Yes please! I'm fairly new at Rust, so any and all feedback is more than welcome!
Feel free to leave pull requests, create issues, or mail me directly at fw@hardijzer.nl

## End goal ?
Something to manage your Zigbee devices and make them available to your Home automation software of choice.
I'm aiming for a mix between AqaraHub and zigbee2mqtt.

## Why is this not using Z-Stack ?
I believe my previous attempt at a Zigbee bridge is limited by the fact that all the important stuff is being handled by the proprietary Z-Stack running on severely underpowered microcontroller.
An example would be IKEA Tradfri devices, which are communicating directly to end-devices, bypassing the coordinator, and requiring a patched Z-Stack.

## Why Rust ?
I'm a big fan of static typechecking, and love using the compiler to catch as much errors as possible. For AqaraHub I used C++14 as I was used to it, but the compile times were horrible, the binary was huge, and cross-compiling was horrible too.
With this in mind, I felt my best options were Rust, Go, or Typescript. The final choice for Rust was purely personal, and I liked Rust, and felt like actually doing a big project in it at some point.
