"""Real stream boundary tests: fragmentation, absolute deadline, EOF, size and peer binding."""
import json
import os
import socket
import struct
import threading
import time
import unittest
from bridge import receive, send, authenticate

class StreamTests(unittest.TestCase):
    def setUp(self): self.a, self.b = socket.socketpair()
    def tearDown(self): self.a.close(); self.b.close()
    def test_fragmented_and_coalesced_frames(self):
        values = [{'op':'refresh','args':{}}, {'op':'status','args':{}}]
        data = b''.join(struct.pack('!I',len(b))+b for b in [json.dumps(v).encode() for v in values])
        def writer():
            for byte in data: self.a.send(bytes([byte]))
        t=threading.Thread(target=writer);t.start()
        self.assertEqual([receive(self.b,time.monotonic()+1) for _ in values],values);t.join()
    def test_eof_and_bounds(self):
        for size in [0, 8*1024*1024+1]:
            self.a.sendall(struct.pack('!I',size))
            with self.assertRaises(ValueError): receive(self.b,time.monotonic()+1)
        with self.assertRaises(ValueError): send(self.a,{'x':'x'*4096},time.monotonic()+1)
        self.a.sendall(b'\x00\x00');self.a.shutdown(socket.SHUT_WR)
        with self.assertRaises(EOFError): receive(self.b,time.monotonic()+1)
    def test_absolute_deadline_and_backpressure(self):
        self.a.sendall(b'\x00')
        with self.assertRaises(TimeoutError): receive(self.b,time.monotonic()+.03)
        self.a.setsockopt(socket.SOL_SOCKET,socket.SO_SNDBUF,4096)
        with self.assertRaises(TimeoutError): send(self.a,{'x':'x'*1000000},time.monotonic()+.03,limit=2000000)
    def test_exact_peer_token_and_incarnation(self):
        for override in [{}, {'token':'wrong'}, {'incarnation':'retired'}, {'pid':os.getpid()+1}]:
            send(self.a,{'token':'private','pid':os.getpid(),'incarnation':'bound',**override},time.monotonic()+1)
            if override:
                with self.assertRaises(AssertionError): authenticate(self.b,os.getpid(),'private','bound')
            else: self.assertEqual(authenticate(self.b,os.getpid(),'private','bound'),os.getpid())
        send(self.a,{'token':'private','pid':os.getpid(),'incarnation':'bound'},time.monotonic()+1)
        with self.assertRaises(AssertionError): authenticate(self.b,os.getpid()+1,'private','bound')

if __name__=='__main__': unittest.main()
