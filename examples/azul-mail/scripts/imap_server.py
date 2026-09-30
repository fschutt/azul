#!/usr/bin/env python3
"""A small IMAP4rev1 server for testing AzMail, standard library only (not written yet)."""


def encode_mutf7(name):
    raise NotImplementedError


def load_message(path):
    raise NotImplementedError


def make_self_signed_cert(directory):
    raise NotImplementedError


def server_ssl_context(cert_path, key_path):
    raise NotImplementedError


def start(root, host, port, user, password, log_path=None, ssl_context=None, special_use=True):
    raise NotImplementedError
