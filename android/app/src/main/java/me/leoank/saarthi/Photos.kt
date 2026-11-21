package me.leoank.saarthi

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.File

/** A photo put in a prompt (ADR-0071): shrunk for sending, with a thumbnail for the box. */
class Attached(val name: String, val bytes: ByteArray, val thumb: ImageBitmap)

/** The long side a photo is shrunk to: enough for Claude to read a screen or a gel. */
private const val LONG_SIDE = 2048
/** A photo's most bytes as sent; a bigger JPEG is saved again at a lower quality. */
private const val MOST_BYTES = 900_000
/** Together, under the node's 3 MB a prompt (crates/node/src/uploads.rs). */
const val PHOTOS_MOST_BYTES = 3 * 1024 * 1024

/** Reads the photo at [uri], upright, shrinks it and keeps it as JPEG; null if it is not an
 *  image the phone can read. */
suspend fun attach(context: Context, uri: Uri): Attached? = withContext(Dispatchers.IO) {
    val bitmap = runCatching { decode(context, uri) }.getOrNull() ?: return@withContext null
    var quality = 85
    var bytes: ByteArray
    do {
        bytes = ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.JPEG, quality, it) }.toByteArray()
        quality -= 15
    } while (bytes.size > MOST_BYTES && quality >= 40)
    val side = 160f / maxOf(bitmap.width, bitmap.height)
    val thumb = Bitmap.createScaledBitmap(bitmap, (bitmap.width * side).toInt().coerceAtLeast(1), (bitmap.height * side).toInt().coerceAtLeast(1), true)
    Attached(jpegName(displayName(context, uri)), bytes, thumb.asImageBitmap())
}

/** Decoded at most [LONG_SIDE] on its long side; turned upright from Android 9 on. */
private fun decode(context: Context, uri: Uri): Bitmap? {
    if (Build.VERSION.SDK_INT >= 28) {
        return ImageDecoder.decodeBitmap(ImageDecoder.createSource(context.contentResolver, uri)) { decoder, info, _ ->
            val (w, h) = info.size.width to info.size.height
            val scale = minOf(1f, LONG_SIDE.toFloat() / maxOf(w, h))
            decoder.setTargetSize((w * scale).toInt().coerceAtLeast(1), (h * scale).toInt().coerceAtLeast(1))
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        }
    }
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= LONG_SIDE) sample *= 2
    return context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample }) }
}

private fun displayName(context: Context, uri: Uri): String =
    runCatching {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }
    }.getOrNull() ?: "photo"

/** The name with `.jpg` in place of its own extension, as the bytes now are. */
private fun jpegName(name: String): String = name.substringBeforeLast('.').ifBlank { "photo" } + ".jpg"

/** The name the node saves a photo under, less its time prefix: letters, digits, `.`, `-`
 *  and `_` only, never empty or hidden (crates/node/src/uploads.rs `safe_name`). */
fun savedName(name: String): String {
    val base = name.substringAfterLast('/').substringAfterLast('\\')
    val clean = base.map { if (it.isLetterOrDigit() && it.code < 128 || it in ".-_") it else '_' }.take(80).joinToString("").trimStart('.')
    return clean.ifEmpty { "photo.jpg" }
}

/** A new file for the camera to write into, and the address it is handed. */
fun cameraFile(context: Context): Uri {
    val dir = File(context.cacheDir, "camera").apply { mkdirs() }
    // Shots from an earlier time are read already: drop them.
    dir.listFiles()?.forEach { it.delete() }
    val file = File(dir, "camera-${System.currentTimeMillis()}.jpg")
    return FileProvider.getUriForFile(context, "${context.packageName}.files", file)
}
