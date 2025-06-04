# My Work with Demikernel and the Cohort Rust library

## Important links:

Cohort (translation to rust) repo: https://github.com/J-avery32/Cohort

The above link is a translation of these files: 

[cohort_aes_base.c](https://github.com/pengwing-project/cohort-private/blob/cohort/piton/verif/diag/c/riscv/ariane/cohort_linux/cohort_aes_base.c)

[cohort_uapi.h](https://github.com/pengwing-project/cohort-private/blob/cohort/piton/verif/diag/assembly/include/riscv/ariane/cohort_uapi.h)


Plain Demikernel ported to RISC-V: https://github.com/pengwing-project/demikernel-private/tree/riscv-demikernel

Demikernel with UDP acceleration: https://github.com/pengwing-project/demikernel-private/tree/josh-udp-cohort

Demikernel with IP acceleration: https://github.com/pengwing-project/demikernel-private/tree/josh-ip-cohort



Ask me questions in the demikernel channel of the openpiton Zulip. My Zulip profile: https://openpiton.zulipchat.com/#user/707651

Ask Professor Balkind for access to my defense slides presentation which provide a high-level overview of my efforts, as well as background of the existing work this is based on.

## Notes before we begin

Before you begin to add on to the code I recommend finding all of my TODOs. These will help you avoid problems and look at things that need to be improved. Although I will try my best to lay that all out here.

## Proper setup with buildroot
Follow the instructions here: https://gist.github.com/NazerkeT/21f716beb8239f7c79145b1a74985918#osbuildroot-image
specifically in the "OS/buildroot" section. Except you need to use this option instead:

`BR2_LINUX_KERNEL_CUSTOM_TARBALL_LOCATION="https://github.com/cohort-project/linux/archive/a-vinod_ariane_cohort_6.2.tar.gz"`

This ensures that the syscall is the correct version that does all the Cohort setup for you when you call it.

In the `rootfs` folder you will want to add the files I have [here](https://github.com/J-avery32/cohort-buildroot-rootfs), along with any binaries you want to test. This will add these files to the root folder when you load your `fw_payload.bin` on the FPGA.

You should be setting this up on the lab's shared server, Jura, compiling an entire Linux kernel on your own machine will take forever.

## Compiling Demikernel and getting the benchmark binaries

Compiling Demikernel to RISC-V has been a hassle and there are still improvements to be made. I had to change the `linux.mk` file so that it statically compiles the code as the buildroot linux does not have a dynamic linker.

First of all you will need to change this line:
 
`export RUSTFLAGS += -Clinker=/home/j/school/596_balkind/riscv64-linux/riscv/bin/riscv64-unknown-linux-gnu-gcc` 

in the top level `linux.mk` file to point to your own riscv compiler. Take a look at this github repo for getting your own riscv toolchain: https://github.com/riscv-collab/riscv-gnu-toolchain. 

Second of all I have not figured out how to get the `all-examples` section of the `linux.mk` to compile. It throws up an error about rust macros, so I commented it out. I suspect it could be an issue with this line in the `linux.mk` file:

```
export LD_LIBRARY_PATH ?= $(CURDIR)/lib:$(shell find $(PREFIX)/lib/ -name '*x86_64-linux-gnu*' -type d 2> /dev/null | xargs | sed -e 's/\s/:/g')
```

In order to compile follow the setup steps and the building steps in the demikernel repo. You don't need to install artifacts like it instructs you to do, and we are only compiling catpowder so you don't need to follow catnip specific steps. Make sure to run `make LIBOS=catpowder` everytime you want to build so that catpowder is built.

The benchmark source code is located in the `tests/rust/udp-tests/bind/mod.rs` in the `loopback_test` function. However, the benchmark binary code will be here after compilation: `target/riscv64gc-unknown-linux-gnu/release/deps/udp_tests-<some-string-of-characters>`. You will need to copy this to your `rootfs` folder in buildroot to run these benchmarks.

### Running the benchmarks

When you get the linux kernel from buildroot running on the fpga you will need to first run `./init_env.sh` which sets up everything correctly. Then you will run `./udp_tests-<some-string-of-characters> --local-address 127.0.0.1:8080 --remote-address 127.0.0.1:8080`. Note that currently there is a bug with the Rust Cohort library where it does not properly clean up after it's `Drop` trait is called. This means that if you want to run the benchmark again you have to load the `fw_payload.bin` onto the FPGA again.

In order to get accurate benchmarks you will need to go into the kernel source in buildroot and remove the kernel prints for the that will print: "MMU flush called at: %llx and %llx\n". This is a link to the file in github that contains this code: https://github.com/cohort-project/linux/blob/b1bc3e3f4e4b257cf7619666a2242c47cb374414/drivers/cohort_mmu/dcpn_compressed.h.

## Cohort Rust library
This library translates the SPSC queues that are used by the Cohort engine into Rust. The sender queue is for sending elements to the hardware accelerator, and the receiver queue is for receiving processed data from the hardware accelerator.

Especially note the TODOs in this library, these are things that need to be improved upon.

## Demikernel

For IP see `src/rust/inetstack/protocols/layer3/mod.rs` for the integration with cohort.

For UDP see `src/rust/inetstack/protocols/layer4/udp/peer.rs` for the integration with cohort.

Note that for accurate benchmarks you will need to have the correct bitstreams loaded onto the fpga.

For IP use this bitstream on Jura: `/home/joshuaavery/tcl/ip_u200.bit`

For UDP use this bitstream: `/home/joshuaavery/tcl/udp_u200.bit`

Current issues that need to be investigated:

Wrap-around issues with the queues, data won't wrap around to the beginning of the queue. This could be an issue with the queue size not being a power of 2, see this [conversation](https://openpiton.zulipchat.com/#narrow/channel/320359-pengwing/topic/beehive/near/520333776) (May 25th 2025 in the beehive topic of the #pengwing channel) on Zulip.

The entire kernel seems to freeze when I attempt to benchmark 20 rounds of 512+ byte packets. When I put print statements in between some of the pushing and popping this seems to inconsistently fix it. Could also be an issue with queue size not being a power of 2. See [here](https://openpiton.zulipchat.com/#narrow/channel/320359-pengwing/topic/beehive/near/520333880) (May 25th 2025 in the beehive topic of the #pengwing channel) on Zulip.

Currently we will just block if no packets are on the receiver queue which is terrible design. Look into integrating the popping logic with Demikernel's polling where it will poll for packets and receive them into an internal queue, from which we pop off in the pop function in `src/rust/inetstack/protocols/layer4/udp/peer.rs`. See the `poll_once` function in `src/rust/inetstack/protocols/layer4/mod.rs`. (See this branch for how it was originally done in `src/rust/inetstack/protocols/layer4/udp/peer.rs`: https://github.com/pengwing-project/demikernel-private/tree/riscv-demikernel). This only needs to be done for UDP, IP has already been integrated into this polling design as it is one level beneath all these details.


Good luck!
