package org.github.salpadding.wasmer

import kotlin.concurrent.withLock


internal class InstanceImpl(private val descriptor: Int) : Instance {
    override val id: Int
        get() = descriptor

    override fun getMemory(off: Int, len: Int): ByteArray {
        if (off < 0 || len < 0) {
            throw RuntimeException("off or len shouldn't be negative")
        }
        return Natives.getMemory(this.descriptor, off, len)
    }

    override fun setMemory(off: Int, buf: ByteArray) {
        if (off < 0) {
            throw RuntimeException("off shouldn't be negative")
        }
        Natives.setMemory(this.descriptor, off, buf)
    }

    override fun execute(export: String, args: LongArray): LongArray {
        return Natives.execute(descriptor, export, args)
    }

    override fun close() {
        Natives.GLOBAL_LOCK.withLock {
            Natives.HOSTS[descriptor] = null
            Natives.close(descriptor)
        }
    }
}