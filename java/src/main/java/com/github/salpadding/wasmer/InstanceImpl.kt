package com.github.salpadding.wasmer

import kotlin.concurrent.withLock


internal class MemoryImpl(var descriptor: Long): Memory {
    override fun read(off: Int, len: Int): ByteArray {
        if (off < 0 || len < 0) {
            throw RuntimeException("off or len shouldn't be negative")
        }
        return Natives.getMemory(this.descriptor, off, len)
    }

    override fun write(off: Int, buf: ByteArray) {
        if (off < 0) {
            throw RuntimeException("off shouldn't be negative")
        }
        Natives.setMemory(this.descriptor, off, buf)
    }
}

internal class InstanceImpl : Instance {
    var descriptor: Long = 0
    var id: Int = 0

    var mem: Memory? = null

    override fun getMemory(name: String): Memory {
        return mem!!
    }

    override fun execute(export: String, args: LongArray): LongArray {
        return Natives.execute(descriptor, export, args)
    }

    override fun close() {
        Natives.close(descriptor)
        Natives.MUTEX.withLock {
            Natives.INSTANCES[this.id] = null
            Natives.HOST_FUNCTIONS[this.id] = null
        }
    }
}