package org.github.salpadding.tinywasm;

public class InstanceImpl implements Instance{
    private final int descriptor;

    public InstanceImpl(int descriptor) {
        this.descriptor = descriptor;
    }

    @Override
    public byte[] getMemory(int off, int len) {
//        return Natives.getMemory(this.descriptor, off, len);
        return null;
    }

    @Override
    public void setMemory(int off, byte[] buf) {
//        Natives.setMemory(this.descriptor, off, buf);
    }

    @Override
    public long[] execute(String export, long[] args) {
        return Natives.execute(this.descriptor, export, args);
    }

    @Override
    public void close() throws Exception {
//        Natives.close(this.descriptor);
    }
}
