# Lab2: Batch Processing and Priviledges

In lab 1, we have made our code work on a **bare-metal computer** (simulated by QEMU) successfully. However, it can do nothing but print some strings we hardcoded in the program on the terminal. Of course you can make it more complicated, such as factoring a large number, calculating the inverse of a matrix, etc. That's cool but there are two significant drawbacks of this approach:

1. The CPU run a single program each time. Since the computing resuorces are precious(in the old time you don't have a modern OS), users who have many programs to run have to wait infront of the computer and manully load&start the next program after previous one finished. Such a labour work.
2. Nobody wants to write the SBI and assembly level stuff everytime, and it's totally duplication of efforts.

In order to solve these problems, people invented the `Simple Batch Processing System`, which can load a batch of application programs and automatically execute them one by one. Besides, the Batch Processing System will provide some "library" code such as console output functions which may be reused by many programs. 

A new problem arises when we use the batch process system: error handling. User's program may (often) run into errors, unconsciously or intentionally. We do not want the error of any program affect others or the system, so the system should be able to hanble errors and terminate the programs if necessary. To achieve this goal we introduced the `Priviledges mechanism` and isolate user's code from system, which we will refered to as usermode and kernelmode. Note that this mechanism requires the support from hardware, and we will illustrate that with code in the following parts.

## 0x00 Usermode Application

Before dive into the system, first we modify the user's application code to usermode which rely on syscalls.