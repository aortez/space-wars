"""Lossless atomic compressed evidence; never overwrite a checkpoint or rollout."""
import gzip
import io
import json
import os
from pathlib import Path
import tempfile
import torch


def write_bytes(path,data):
    path=Path(path)
    if path.exists():raise FileExistsError(path)
    with tempfile.NamedTemporaryFile(dir=path.parent,delete=False) as f:
        f.write(gzip.compress(data,compresslevel=1,mtime=0));f.flush();os.fsync(f.fileno());temporary=f.name
    os.replace(temporary,path)


def dump_json(path,data):
    write_bytes(path,json.dumps(data,allow_nan=False,separators=(',',':')).encode())


def read_json(path):
    path=Path(path);data=path.read_bytes()
    return json.loads(gzip.decompress(data) if path.suffix=='.gz' else data)


def dump_torch(path,data):
    buffer=io.BytesIO();torch.save(data,buffer);write_bytes(path,buffer.getvalue())


def load_torch(path):
    path=Path(path)
    if path.suffix!='.gz':return torch.load(path,weights_only=True)
    return torch.load(io.BytesIO(gzip.decompress(path.read_bytes())),weights_only=True)
