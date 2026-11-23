package io.github.m_compressors.mbook;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;

/**
 * Receives the documents other apps open or share with mbook, and brings
 * mbook up with them.
 *
 * Another app may start the activity that receives a document in its own
 * task, and for a result, as Telegram does; a second {@link MbookActivity}
 * there cannot run the interface, which already runs in the first one, and
 * closes at once. This activity has no window and no native code: it queues
 * the documents, starts {@code MbookActivity} in its own task, where the
 * running interface comes to the front, and finishes.
 */
public final class OpenActivity extends Activity {
    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (state == null) {
            MbookActivity.receive(this, getIntent());
        }
        startActivity(new Intent(this, MbookActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        finish();
    }
}
