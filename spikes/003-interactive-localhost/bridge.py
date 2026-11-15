"""Bounded framing for one explicitly bound fixture Unix stream; no reconnect/replay."""
import json
import os
import socket
import struct
import time

REQUEST_LIMIT = 4096
REPLY_LIMIT = 8 * 1024 * 1024

def authenticate(sock, expected_pid, token, incarnation):
    peer_pid, peer_uid, _ = struct.unpack('3i', sock.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
    hello = receive(sock, time.monotonic()+8, 4096)
    assert peer_pid == expected_pid and peer_uid == os.getuid(), 'wrong native OS peer'
    assert hello == {'token':token, 'pid':expected_pid, 'incarnation':incarnation}, 'wrong native owner/incarnation'
    return peer_pid

def receive(sock, deadline, limit=REPLY_LIMIT):
    def exact(size):
        result = bytearray()
        while len(result) < size:
            left = deadline - time.monotonic()
            if left <= 0:
                raise TimeoutError('bridge absolute deadline')
            sock.settimeout(left)
            chunk = sock.recv(size - len(result))
            if not chunk:
                raise EOFError('bridge EOF; outcome may be ambiguous; never replay')
            result.extend(chunk)
        return result
    size = struct.unpack('!I', exact(4))[0]
    if not 0 < size <= limit:
        raise ValueError('bridge frame length')
    return json.loads(exact(size))

def send(sock, value, deadline, limit=REQUEST_LIMIT):
    body = json.dumps(value, separators=(',', ':')).encode()
    if not 0 < len(body) <= limit:
        raise ValueError('bridge frame length')
    data = memoryview(struct.pack('!I', len(body)) + body)
    while data:
        left = deadline - time.monotonic()
        if left <= 0:
            raise TimeoutError('bridge absolute deadline')
        sock.settimeout(left)
        count = sock.send(data)
        if not count:
            raise EOFError('bridge write EOF; never replay')
        data = data[count:]
