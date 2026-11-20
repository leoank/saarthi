package me.leoank.saarthi

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext

/** Dictation into the prompt (ADR-0070): Android's own recognizer, on the device where the
 *  phone has one. A tap starts it; a second tap, or a pause, ends it, and the words go to
 *  [heard]. Nothing is sent by itself. */
class Dictation(private val context: Context, private val heard: (String) -> Unit) {
    var listening by mutableStateOf(false)
        private set
    /** The words so far, while listening. */
    var partial by mutableStateOf("")
        private set
    /** How loud the voice is, 0 to 1, for the mic's ring. */
    var level by mutableStateOf(0f)
        private set
    /** Why the last try gave nothing, until the next. */
    var problem by mutableStateOf<String?>(null)
        private set

    fun clearProblem() {
        problem = null
    }

    val available: Boolean = SpeechRecognizer.isRecognitionAvailable(context)
    private var recognizer: SpeechRecognizer? = null

    fun start() {
        if (listening) return
        val r = recognizer ?: create().also { recognizer = it }
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            putExtra(RecognizerIntent.EXTRA_CALLING_PACKAGE, context.packageName)
        }
        partial = ""
        problem = null
        listening = true
        r.startListening(intent)
    }

    /** Ends listening; what was heard still comes. */
    fun stop() = recognizer?.stopListening()

    fun release() {
        recognizer?.destroy()
        recognizer = null
        listening = false
    }

    /** The on-device recognizer where there is one, so the voice stays on the phone. */
    private fun create(): SpeechRecognizer {
        val local = Build.VERSION.SDK_INT >= 31 && SpeechRecognizer.isOnDeviceRecognitionAvailable(context)
        val r = if (local) SpeechRecognizer.createOnDeviceSpeechRecognizer(context) else SpeechRecognizer.createSpeechRecognizer(context)
        r.setRecognitionListener(object : RecognitionListener {
            override fun onReadyForSpeech(params: Bundle?) {}
            override fun onBeginningOfSpeech() {}
            override fun onRmsChanged(rmsdB: Float) {
                level = ((rmsdB + 2f) / 12f).coerceIn(0f, 1f)
            }
            override fun onBufferReceived(buffer: ByteArray?) {}
            override fun onEndOfSpeech() {}
            override fun onError(error: Int) {
                listening = false
                partial = ""
                level = 0f
                problem = when (error) {
                    SpeechRecognizer.ERROR_NO_MATCH, SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> "Heard nothing"
                    SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "No microphone permission"
                    SpeechRecognizer.ERROR_NETWORK, SpeechRecognizer.ERROR_NETWORK_TIMEOUT -> "The recognizer needs the network"
                    SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "The recognizer is busy"
                    SpeechRecognizer.ERROR_CLIENT -> null
                    else -> "The recognizer stopped (error $error)"
                }
            }
            override fun onResults(results: Bundle?) {
                listening = false
                partial = ""
                level = 0f
                results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.takeIf { it.isNotBlank() }?.let(heard)
                    ?: run { problem = "Heard nothing" }
            }
            override fun onPartialResults(partialResults: Bundle?) {
                partialResults?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let { partial = it }
            }
            override fun onEvent(eventType: Int, params: Bundle?) {}
        })
        return r
    }
}

/** A [Dictation] for as long as the caller stays composed. */
@Composable
fun rememberDictation(heard: (String) -> Unit): Dictation {
    val context = LocalContext.current
    val latest by rememberUpdatedState(heard)
    val d = remember { Dictation(context.applicationContext) { latest(it) } }
    DisposableEffect(d) { onDispose { d.release() } }
    return d
}

/** [words] put into [text] at [at], with a space on either side where it would run on. */
fun insertWords(text: String, at: Int, words: String): Pair<String, Int> {
    val i = at.coerceIn(0, text.length)
    val before = if (i > 0 && !text[i - 1].isWhitespace()) " " else ""
    val after = if (i < text.length && !text[i].isWhitespace()) " " else ""
    val put = before + words.trim() + after
    return text.substring(0, i) + put + text.substring(i) to i + put.length - after.length
}
