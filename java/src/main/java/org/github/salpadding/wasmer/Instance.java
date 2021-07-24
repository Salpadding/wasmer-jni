package org.github.salpadding.wasmer;

public interface Instance extends AutoCloseable{
    byte[] getMemory(int off, int len);
    void setMemory(int off, byte[] buf);
    long[] execute(String export, long[] args);
}
