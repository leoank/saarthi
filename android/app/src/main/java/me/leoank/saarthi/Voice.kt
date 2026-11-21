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

/** Dictation into the prompt (ADR-0070, ADR-0073): Android's own recognizer, on the device
 *  where the phone has one, for as long as the mic is held. The recognizer ends at each
 *  pause; while held, what it heard goes to [heard] and it listens again at once, so a pause
 *  to think does not end the dictation. Letting go ends it. Nothing is sent by itself. */
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

    /** Whether the mic is held down. */
    private var held = false
    /** Whether anything was heard since the mic was pressed. */
    private var heardAny = false

    /** The mic pressed: listening starts. */
    fun hold() {
        held = true
        heardAny = false
        problem = null
        listen()
    }

    /** The mic let go: listening ends, and what was heard so far still comes. */
    fun letGo() {
        if (!held) return
        held = false
        if (listening) recognizer?.stopListening()
    }

    private fun listen() {
        val r = recognizer ?: create().also { recognizer = it }
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            putExtra(RecognizerIntent.EXTRA_CALLING_PACKAGE, context.packageName)
            // Longer pauses before it ends, where the recognizer heeds them.
            putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_COMPLETE_SILENCE_LENGTH_MILLIS, 6000L)
            putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_POSSIBLY_COMPLETE_SILENCE_LENGTH_MILLIS, 6000L)
        }
        partial = ""
        listening = true
        r.startListening(intent)
    }

    /** A stretch of listening ended: while held, listen again. */
    private fun ended() {
        partial = ""
        level = 0f
        if (held) listen() else listening = false
    }

    fun release() {
        held = false
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
                // A pause with nothing said, while still held: listen on.
                val silence = error == SpeechRecognizer.ERROR_NO_MATCH || error == SpeechRecognizer.ERROR_SPEECH_TIMEOUT
                if (silence && held) return ended()
                held = false
                ended()
                problem = when (error) {
                    SpeechRecognizer.ERROR_NO_MATCH, SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> if (heardAny) null else "Heard nothing"
                    SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "No microphone permission"
                    SpeechRecognizer.ERROR_NETWORK, SpeechRecognizer.ERROR_NETWORK_TIMEOUT -> "The recognizer needs the network"
                    SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "The recognizer is busy"
                    SpeechRecognizer.ERROR_CLIENT -> null
                    else -> "The recognizer stopped (error $error)"
                }
            }
            override fun onResults(results: Bundle?) {
                val words = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.takeIf { it.isNotBlank() }
                if (words != null) {
                    heardAny = true
                    heard(words)
                }
                ended()
                if (!listening && !heardAny) problem = "Heard nothing"
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
