package com.archeros.wasmer;

public interface Memory {
    byte[] read(int off, int len);

    void write(int off, byte[] buf);
}
