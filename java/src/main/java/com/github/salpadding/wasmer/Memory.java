package com.github.salpadding.wasmer;

public interface Memory {
    byte[] read(int off, int len);

    void write(int off, byte[] buf);
}
