package com.github.salpadding.wasmer;


class MemoryImpl implements Memory {
    long descriptor;

    public MemoryImpl(long desc) {
        this.descriptor = desc;
    }

    public byte[] read(int off, int len) {
        if (off < 0 || len < 0) {
            throw new RuntimeException("off or len shouldn't be negative");
        }
        return Natives.getMemory(this.descriptor, off, len);
    }

    public void write(int off, byte[] buf) {
        if (off < 0) {
            throw new RuntimeException("off shouldn't be negative");
        }
        Natives.setMemory(this.descriptor, off, buf);
    }
}

class InstanceImpl implements Instance {
    long descriptor;
    int id;
    Memory mem;


    public Memory getMemory(String name) {
        return mem;
    }


    public long[] execute(String export, long[] args) {
        return Natives.execute(descriptor, export, args);
    }

    public void close() {
        Natives.close(descriptor);

        Natives.MUTEX.lock();

        try {
            Natives.INSTANCES[this.id] = null;
            Natives.HOST_FUNCTIONS[this.id] = null;
        } finally {
            Natives.MUTEX.unlock();
        }
    }
}