# SwapBytes

SwapBytes is a simple peer to peer file sharing application that allows users to chat in public rooms, create private messaging sessions, and share files.

## How to run

To run the application, type the following into a terminal at the root of the directory:

```bash
cargo run
```

To run multiple peers, run multiple terminal instances.  
Note that this application discovers peers through a local network.

## How to use

First, you must have more than one peer connected, and choose a nickname. From there you are brought to the Global chat topic where you can chat with others.

### Changing Rooms

To change rooms, switch tabs by pressing tab. You can then select a room using the arrow keys and pressing enter.

### Commands

The application has multiple commands that the user can use to perform different actions.

_Note that when using a command with an argument, you do not need to provide the square brackets. They are there to show that it is a variable._

Below is a list of the available commands:

**/help** - _View a list of all available commands_  
**/list** - _List all known users that have sent a message_  
**/create_room [room]** - _Create a new room and join it_  
**/connect [nickname]** - _Invite a peer to share files and chat privately_  
**/request [filename]** - _Request a file in a private messaging session_  
**/accept** - _Accept an incoming request (such as a file, or a connection)_  
**/reject** - _Reject an incoming request (such as a file, or a connection)_  
**/leave** - _Leave a private messaging session_
