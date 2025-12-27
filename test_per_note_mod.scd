// SuperCollider test script for per-note modulation with clap-osc-host
// Run clap-osc-host first:
//   ./target/release/clap-osc-host "/Library/Audio/Plug-Ins/CLAP/Surge XT.clap" --osc-port 9000

(
// Configuration
~oscPort = 9000;
~oscAddr = NetAddr("127.0.0.1", ~oscPort);

// Surge XT per-note modulatable parameters
~filterCutoffId = 4092842705;  // A Filter 1 Cutoff
~filterResonanceId = 8095466;  // A Filter 1 Resonance
~pitchId = 3014957538;  // A Pitch - use this for obvious vibrato test

// Helper functions
~noteOn = { |noteId, key, vel=0.8, chan=0, port=0|
    ~oscAddr.sendMsg("/note/on", noteId, key, vel, chan, port);
    "Note ON: id=% key=% vel=%".format(noteId, key, vel).postln;
};

~noteOff = { |noteId, key, vel=0.0, chan=0, port=0|
    ~oscAddr.sendMsg("/note/off", noteId, key, vel, chan, port);
    "Note OFF: id=% key=%".format(noteId, key).postln;
};

~paramMod = { |noteId, paramId, amount, key=(-1), chan=(-1), port=(-1)|
    ~oscAddr.sendMsg("/param/mod", noteId, paramId, amount, key, chan, port);
};

~paramSet = { |paramId, value|
    ~oscAddr.sendMsg("/param/set", paramId, value);
    "Param SET: id=% value=%".format(paramId, value).postln;
};

~dumpPatchState = {
    ~oscAddr.sendMsg("/patchState");
    "Requested patch state dump".postln;
};

// Task 1: Low note with slow LFO on filter cutoff
~task1 = Task({
    var noteId = 1;
    var key = 48;  // C3
    var lfoPhase = 0;
    var lfoRate = 0.5;  // Hz - slow wobble

    "Task 1 starting: note % with slow filter LFO".format(key).postln;
    ~noteOn.(noteId, key, 0.7);

    loop {
        // Calculate LFO value - use pitch for obvious vibrato effect
        var modAmount = sin(lfoPhase * 2pi) * 0.05;  // Small amount for pitch (semitones)
        ~paramMod.(noteId, ~pitchId, modAmount);

        lfoPhase = (lfoPhase + (lfoRate * 0.02)) % 1.0;
        0.02.wait;  // 50Hz update rate
    };
});

// Task 2: High note with fast LFO on filter cutoff
~task2 = Task({
    var noteId = 2;
    var key = 72;  // C5
    var lfoPhase = 0;
    var lfoRate = 3.0;  // Hz - fast wobble

    "Task 2 starting: note % with fast filter LFO".format(key).postln;
    ~noteOn.(noteId, key, 0.7);

    loop {
        // Calculate LFO value - faster vibrato on high note
        var modAmount = sin(lfoPhase * 2pi) * 0.08;
        ~paramMod.(noteId, ~pitchId, modAmount);

        lfoPhase = (lfoPhase + (lfoRate * 0.02)) % 1.0;
        0.02.wait;  // 50Hz update rate
    };
});

// Start both tasks
~startTasks = {
	Task({
		// Set initial filter cutoff to middle value
		~paramSet.(~filterCutoffId, 0.5);
		0.1.wait;
		
		~task1.reset.play;
		~task2.reset.play;
		"Both tasks running - each note has independent filter modulation".postln;
	}).play
};

// Stop both tasks and release notes
~stopTasks = {
    ~task1.stop;
    ~task2.stop;
    ~noteOff.(1, 48);
    ~noteOff.(2, 72);
    "Tasks stopped, notes released".postln;
};

"=== Per-Note Modulation Test Script ===".postln;
"Run ~startTasks.() to start".postln;
"Run ~stopTasks.() to stop".postln;
)

// Execute this line to start:
~startTasks.()

// Execute this line to stop:
~stopTasks.()
