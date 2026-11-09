// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.nexoryx.pasteget

import android.content.ClipData
import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import uniffi.pasteget_ffi.PastegetException
import uniffi.pasteget_ffi.PhoneClient
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer

/** Sends text and images to the connected computers. Every function returns a message for the user. */
object Sender {
    /** How long to wait for a connection when the service was just started. */
    private const val CONNECT_WAIT_MS = 5_000L

    /** Sends the first item of [clip]: its text, or the image it points to. */
    suspend fun send(context: Context, clip: ClipData?): String {
        val item = clip?.takeIf { it.itemCount > 0 }?.getItemAt(0)
            ?: return context.getString(R.string.clipboard_empty)
        val uri = item.uri
        val mimeType = uri?.let { context.contentResolver.getType(it) }
        return when {
            uri != null && mimeType?.startsWith("image/") == true -> sendImage(context, uri)
            item.text != null -> sendText(context, item.text.toString())
            else -> context.getString(R.string.unsupported_content)
        }
    }

    suspend fun sendText(context: Context, text: String): String =
        deliver(context, ActivityKind.TEXT, text.encodeToByteArray().size) { it.sendText(text) }

    suspend fun sendImage(context: Context, uri: Uri): String = try {
        val image = withContext(Dispatchers.IO) { readImage(context, uri) }
        deliver(context, ActivityKind.IMAGE, image.size) { it.sendImage(image) }
    } catch (e: Exception) {
        context.getString(R.string.send_failed, e.message ?: e.javaClass.simpleName)
    }

    /** Sends [bytes] of [kind] with [send], and notes it in the activity log. */
    private suspend fun deliver(context: Context, kind: ActivityKind, bytes: Int, send: (PhoneClient) -> UInt): String {
        val client = connectedClient(context)
        val sentTo = try {
            withContext(Dispatchers.IO) { send(client).toInt() }
        } catch (e: PastegetException) {
            return context.getString(R.string.send_failed, e.message)
        }
        context.pasteGet.activity.sent(kind, bytes, sentTo, automatic = false)
        if (sentTo > 0) return resultText(context, sentTo, emptyList())
        val paired = withContext(Dispatchers.IO) { client.pairedComputers().map { it.name } }
        return resultText(context, sentTo, paired)
    }

    /**
     * What to say once a clip went to [sentTo] connected computers. When none is connected, it
     * waits for the [paired] ones (by name) and goes as soon as one is back: what's shared by
     * hand is never sensitive, and only sensitive clips don't wait.
     */
    fun resultText(context: Context, sentTo: Int, paired: List<String>): String = when {
        sentTo > 0 -> context.resources.getQuantityString(R.plurals.sent_to, sentTo, sentTo)
        paired.isEmpty() -> context.getString(R.string.notification_no_computer)
        paired.size == 1 -> context.getString(R.string.send_waits_for, paired.single())
        else -> context.getString(R.string.send_waits)
    }

    /** Makes sure the connection service runs, and gives it a moment to connect if it just started. */
    private suspend fun connectedClient(context: Context): PhoneClient {
        val client = context.pasteGet.client
        if (client.connectedCount() > 0u) return client
        SyncService.start(context)
        var waited = 0L
        while (client.connectedCount() == 0u && waited < CONNECT_WAIT_MS) {
            delay(100)
            waited += 100
        }
        return client
    }

    private fun readImage(context: Context, uri: Uri): ByteArray {
        val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
            ?: error("could not read the image")
        return sendable(bytes)
    }

    /**
     * PNG and JPEG bytes as they are (re-encoding a screenshot would cost more time than sending
     * it), other formats (WebP, HEIC…) re-encoded as PNG.
     */
    fun sendable(bytes: ByteArray): ByteArray {
        if (Images.isPng(bytes) || Images.isJpeg(bytes)) return bytes
        // createSource(ByteBuffer) exists from API 28; the ByteArray overload only from API 31.
        val bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, _, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        }
        return ByteArrayOutputStream().use { out ->
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)
            out.toByteArray()
        }
    }
}
