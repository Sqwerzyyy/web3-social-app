// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.10;

contract GuestBook{
    struct Message{
        string text;
        address writer;
    }
    Message[] public messages;
    function leaveMessage(string memory _text) public {
        messages.push(Message(_text, msg.sender));
    }

}
