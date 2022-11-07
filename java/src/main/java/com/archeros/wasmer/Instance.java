package com.archeros.wasmer;

import java.util.Collection;


/**
 * Instance is not thread safe, dont share Instance object between threads
 */
public interface Instance extends AutoCloseable {
    long[] EMPTY_LONGS = new long[0];

    /**
     * create new instance by webassembly byte code, and open options
     */
    static Instance create(byte[] bin, Options options, Collection<HostFunction> hosts) {
        String[] names = hosts == null ? new String[0] : hosts.stream().map(HostFunction::getName).toArray(String[]::new);
        HostFunction[] hostsArray = hosts == null ? new HostFunction[0] : hosts.toArray(new HostFunction[0]);
        byte[][] sigs = hosts == null ? new byte[0][] :
                hosts.stream().map(x -> Natives.encodeSignature(x.getParams(), x.getRet()))
                        .toArray(byte[][]::new);

        InstanceImpl ins = new InstanceImpl();
        int insId = -1;

        Natives.MUTEX.lock();
        try {
            for (int i = 0; i < Natives.INSTANCES.length; i++) {
                if (Natives.INSTANCES[i] == null) {
                    Natives.INSTANCES[i] = ins;
                    insId = i;
                    break;
                }
            }
        } finally {
            Natives.MUTEX.unlock();
        }

        if (insId < 0) {
            throw new RuntimeException("failed to create instance, consider close some instances");
        }

        ins.id = insId;
        Natives.HOST_FUNCTIONS[insId] = hostsArray;

        long descriptor = Natives.createInstance(bin, options.bitmap(), insId, names, sigs);
        ins.descriptor = descriptor;
        ins.mem = new MemoryImpl(descriptor);
        return ins;
    }

    Memory getMemory(String name);

    /**
     * execute exported function
     */
    long[] execute(String export, long[] args);

    @Override
    void close();
}
