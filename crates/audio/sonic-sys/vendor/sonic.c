/* Sonic library
   Copyright 2010
   Bill Cox
   This file is part of the Sonic Library.

   This file is licensed under the Apache 2.0 license.
*/

#include "sonic.h"

#include <limits.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>





































#define CLAMP(val, min, max) \
  ((val) < (min) ? (min) : (val) > (max) ? (max) : (val))


#define SINC_FILTER_POINTS \
  12 
#define SINC_TABLE_SIZE 601


static short sincTable[SINC_TABLE_SIZE] = {
    0,     0,     0,     0,     0,     0,     0,     -1,    -1,    -2,    -2,
    -3,    -4,    -6,    -7,    -9,    -10,   -12,   -14,   -17,   -19,   -21,
    -24,   -26,   -29,   -32,   -34,   -37,   -40,   -42,   -44,   -47,   -48,
    -50,   -51,   -52,   -53,   -53,   -53,   -52,   -50,   -48,   -46,   -43,
    -39,   -34,   -29,   -22,   -16,   -8,    0,     9,     19,    29,    41,
    53,    65,    79,    92,    107,   121,   137,   152,   168,   184,   200,
    215,   231,   247,   262,   276,   291,   304,   317,   328,   339,   348,
    357,   363,   369,   372,   374,   375,   373,   369,   363,   355,   345,
    332,   318,   300,   281,   259,   234,   208,   178,   147,   113,   77,
    39,    0,     -41,   -85,   -130,  -177,  -225,  -274,  -324,  -375,  -426,
    -478,  -530,  -581,  -632,  -682,  -731,  -779,  -825,  -870,  -912,  -951,
    -989,  -1023, -1053, -1080, -1104, -1123, -1138, -1149, -1154, -1155, -1151,
    -1141, -1125, -1105, -1078, -1046, -1007, -963,  -913,  -857,  -796,  -728,
    -655,  -576,  -492,  -403,  -309,  -210,  -107,  0,     111,   225,   342,
    462,   584,   708,   833,   958,   1084,  1209,  1333,  1455,  1575,  1693,
    1807,  1916,  2022,  2122,  2216,  2304,  2384,  2457,  2522,  2579,  2625,
    2663,  2689,  2706,  2711,  2705,  2687,  2657,  2614,  2559,  2491,  2411,
    2317,  2211,  2092,  1960,  1815,  1658,  1489,  1308,  1115,  912,   698,
    474,   241,   0,     -249,  -506,  -769,  -1037, -1310, -1586, -1864, -2144,
    -2424, -2703, -2980, -3254, -3523, -3787, -4043, -4291, -4529, -4757, -4972,
    -5174, -5360, -5531, -5685, -5819, -5935, -6029, -6101, -6150, -6175, -6175,
    -6149, -6096, -6015, -5905, -5767, -5599, -5401, -5172, -4912, -4621, -4298,
    -3944, -3558, -3141, -2693, -2214, -1705, -1166, -597,  0,     625,   1277,
    1955,  2658,  3386,  4135,  4906,  5697,  6506,  7332,  8173,  9027,  9893,
    10769, 11654, 12544, 13439, 14335, 15232, 16128, 17019, 17904, 18782, 19649,
    20504, 21345, 22170, 22977, 23763, 24527, 25268, 25982, 26669, 27327, 27953,
    28547, 29107, 29632, 30119, 30569, 30979, 31349, 31678, 31964, 32208, 32408,
    32565, 32677, 32744, 32767, 32744, 32677, 32565, 32408, 32208, 31964, 31678,
    31349, 30979, 30569, 30119, 29632, 29107, 28547, 27953, 27327, 26669, 25982,
    25268, 24527, 23763, 22977, 22170, 21345, 20504, 19649, 18782, 17904, 17019,
    16128, 15232, 14335, 13439, 12544, 11654, 10769, 9893,  9027,  8173,  7332,
    6506,  5697,  4906,  4135,  3386,  2658,  1955,  1277,  625,   0,     -597,
    -1166, -1705, -2214, -2693, -3141, -3558, -3944, -4298, -4621, -4912, -5172,
    -5401, -5599, -5767, -5905, -6015, -6096, -6149, -6175, -6175, -6150, -6101,
    -6029, -5935, -5819, -5685, -5531, -5360, -5174, -4972, -4757, -4529, -4291,
    -4043, -3787, -3523, -3254, -2980, -2703, -2424, -2144, -1864, -1586, -1310,
    -1037, -769,  -506,  -249,  0,     241,   474,   698,   912,   1115,  1308,
    1489,  1658,  1815,  1960,  2092,  2211,  2317,  2411,  2491,  2559,  2614,
    2657,  2687,  2705,  2711,  2706,  2689,  2663,  2625,  2579,  2522,  2457,
    2384,  2304,  2216,  2122,  2022,  1916,  1807,  1693,  1575,  1455,  1333,
    1209,  1084,  958,   833,   708,   584,   462,   342,   225,   111,   0,
    -107,  -210,  -309,  -403,  -492,  -576,  -655,  -728,  -796,  -857,  -913,
    -963,  -1007, -1046, -1078, -1105, -1125, -1141, -1151, -1155, -1154, -1149,
    -1138, -1123, -1104, -1080, -1053, -1023, -989,  -951,  -912,  -870,  -825,
    -779,  -731,  -682,  -632,  -581,  -530,  -478,  -426,  -375,  -324,  -274,
    -225,  -177,  -130,  -85,   -41,   0,     39,    77,    113,   147,   178,
    208,   234,   259,   281,   300,   318,   332,   345,   355,   363,   369,
    373,   375,   374,   372,   369,   363,   357,   348,   339,   328,   317,
    304,   291,   276,   262,   247,   231,   215,   200,   184,   168,   152,
    137,   121,   107,   92,    79,    65,    53,    41,    29,    19,    9,
    0,     -8,    -16,   -22,   -29,   -34,   -39,   -43,   -46,   -48,   -50,
    -52,   -53,   -53,   -53,   -52,   -51,   -50,   -48,   -47,   -44,   -42,
    -40,   -37,   -34,   -32,   -29,   -26,   -24,   -21,   -19,   -17,   -14,
    -12,   -10,   -9,    -7,    -6,    -4,    -3,    -2,    -2,    -1,    -1,
    0,     0,     0,     0,     0,     0,     0};





#ifndef SONIC_NO_MALLOC


static void* sonicCalloc(int num, int size) { return calloc(num, size); }


static void* sonicRealloc(void* p, int oldNum, int newNum, int size) {
  return realloc(p, newNum * size);
}


static void sonicFree(void* p) { free(p); }

#else

#ifndef SONIC_MAX_MEMORY

#define SONIC_MAX_MEMORY (16 * 1024)
#endif





static void*
    memoryBufferAligned[(SONIC_MAX_MEMORY + sizeof(void) - 1) / sizeof(void*)];
static unsigned char* memoryBuffer = (unsigned char*)memoryBufferAligned;
static int memoryBufferPos = 0;


static void* sonicCalloc(int num, int size) {
  int len = num * size;

  if (memoryBufferPos + len > SONIC_MAX_MEMORY) {
    return 0;
  }
  unsigned char* p = memoryBuffer + memoryBufferPos;
  memoryBufferPos += len;
  memset(p, 0, len);
  return p;
}



static void* sonicRealloc(void* p, int oldNum, int newNum, int size) {
  if (newNum <= oldNum) {
    return p;
  }
  void* newBuffer = sonicCalloc(newNum, size);
  if (newBuffer == NULL) {
    return NULL;
  }
  memcpy(newBuffer, p, oldNum * size);
  return newBuffer;
}



static void sonicFree(void* p) { memoryBufferPos = 0; }

#endif

struct sonicStreamStruct {
#ifdef SONIC_SPECTROGRAM
  sonicSpectrogram spectrogram;
#endif 
  short* inputBuffer;
  short* outputBuffer;
  short* pitchBuffer;
  short* downSampleBuffer;
  void* userData;
  float speed;
  float volume;
  float pitch;
  float rate;
  














  float samplePeriod; 
  
  float inputPlayTime;
  

  float timeError;
  int oldRatePosition;
  int newRatePosition;
  int quality;
  int numChannels;
  int inputBufferSize;
  int pitchBufferSize;
  int outputBufferSize;
  int numInputSamples;
  int numOutputSamples;
  int numPitchSamples;
  int minPeriod;
  int maxPeriod;
  int maxRequired;
  int remainingInputToCopy;
  int sampleRate;
  int prevPeriod;
  int prevMinDiff;
};


void sonicSetUserData(sonicStream stream, void* userData) {
  stream->userData = userData;
}


void* sonicGetUserData(sonicStream stream) { return stream->userData; }

#ifdef SONIC_SPECTROGRAM


void sonicComputeSpectrogram(sonicStream stream) {
  stream->spectrogram = sonicCreateSpectrogram(stream->sampleRate);
  
  sonicSetSpeed(stream, 2.0);
}


sonicSpectrogram sonicGetSpectrogram(sonicStream stream) {
  return stream->spectrogram;
}

#endif


static void scaleSamples(short* samples, int numSamples, float volume) {
  
  int fixedPointVolume = volume * 256.0f;
  int value;

  while (numSamples--) {
    value = (*samples * fixedPointVolume) >> 8;
    if (value > 32767) {
      value = 32767;
    } else if (value < -32767) {
      value = -32767;
    }
    *samples++ = value;
  }
}


float sonicGetSpeed(sonicStream stream) { return stream->speed; }


void sonicSetSpeed(sonicStream stream, float speed) {
  stream->speed = CLAMP(speed, SONIC_MIN_SPEED, SONIC_MAX_SPEED);
}


float sonicGetPitch(sonicStream stream) { return stream->pitch; }


void sonicSetPitch(sonicStream stream, float pitch) {
  stream->pitch = CLAMP(pitch, SONIC_MIN_PITCH_SETTING, SONIC_MAX_PITCH_SETTING);
}


float sonicGetRate(sonicStream stream) { return stream->rate; }



void sonicSetRate(sonicStream stream, float rate) {
  stream->rate = CLAMP(rate, SONIC_MIN_RATE, SONIC_MAX_RATE);

  stream->oldRatePosition = 0;
  stream->newRatePosition = 0;
}


int sonicGetChordPitch(sonicStream stream) { return 0; }



void sonicSetChordPitch(sonicStream stream, int useChordPitch) {}


int sonicGetQuality(sonicStream stream) { return stream->quality; }



void sonicSetQuality(sonicStream stream, int quality) {
  stream->quality = quality != 0? 1 : 0;
}


float sonicGetVolume(sonicStream stream) { return stream->volume; }


void sonicSetVolume(sonicStream stream, float volume) {
  stream->volume = CLAMP(volume, SONIC_MIN_VOLUME, SONIC_MAX_VOLUME);
}


static void freeStreamBuffers(sonicStream stream) {
  if (stream->inputBuffer != NULL) {
    sonicFree(stream->inputBuffer);
  }
  if (stream->outputBuffer != NULL) {
    sonicFree(stream->outputBuffer);
  }
  if (stream->pitchBuffer != NULL) {
    sonicFree(stream->pitchBuffer);
  }
  if (stream->downSampleBuffer != NULL) {
    sonicFree(stream->downSampleBuffer);
  }
}


void sonicDestroyStream(sonicStream stream) {
#ifdef SONIC_SPECTROGRAM
  if (stream->spectrogram != NULL) {
    sonicDestroySpectrogram(stream->spectrogram);
  }
#endif 
  freeStreamBuffers(stream);
  sonicFree(stream);
}


static int computeSkip(sonicStream stream, int sampleRate) {
  int skip = 1;
  if (sampleRate > SONIC_AMDF_FREQ && stream->quality == 0) {
    skip = sampleRate / SONIC_AMDF_FREQ;
  }
  return skip;
}


static int allocateStreamBuffers(sonicStream stream, int sampleRate,
                                 int numChannels) {
  int minPeriod = sampleRate / SONIC_MAX_PITCH;
  int maxPeriod = sampleRate / SONIC_MIN_PITCH;
  int maxRequired = 2 * maxPeriod;

  
  stream->inputBufferSize = maxRequired + (maxRequired >> 2);

  stream->inputBuffer =
      (short*)sonicCalloc(stream->inputBufferSize, sizeof(short) * numChannels);
  if (stream->inputBuffer == NULL) {
    sonicDestroyStream(stream);
    return 0;
  }
  
  stream->outputBufferSize = maxRequired + (maxRequired >> 2);
  stream->outputBuffer = (short*)sonicCalloc(stream->outputBufferSize,
                                             sizeof(short) * numChannels);
  if (stream->outputBuffer == NULL) {
    sonicDestroyStream(stream);
    return 0;
  }
  
  stream->pitchBufferSize = maxRequired + (maxRequired >> 2);
  stream->pitchBuffer =
      (short*)sonicCalloc(stream->pitchBufferSize, sizeof(short) * numChannels);
  if (stream->pitchBuffer == NULL) {
    sonicDestroyStream(stream);
    return 0;
  }
  int downSampleBufferSize = maxRequired;
  stream->downSampleBuffer =
      (short*)sonicCalloc(downSampleBufferSize, sizeof(short));
  if (stream->downSampleBuffer == NULL) {
    sonicDestroyStream(stream);
    return 0;
  }
  stream->sampleRate = sampleRate;
  stream->samplePeriod = 1.0 / sampleRate;
  stream->numChannels = numChannels;
  stream->oldRatePosition = 0;
  stream->newRatePosition = 0;
  stream->minPeriod = minPeriod;
  stream->maxPeriod = maxPeriod;
  stream->maxRequired = maxRequired;
  stream->prevPeriod = 0;
  return 1;
}



sonicStream sonicCreateStream(int sampleRate, int numChannels) {
  sonicStream stream =
      (sonicStream)sonicCalloc(1, sizeof(struct sonicStreamStruct));

  sampleRate = CLAMP(sampleRate, SONIC_MIN_SAMPLE_RATE, SONIC_MAX_SAMPLE_RATE);
  numChannels = CLAMP(numChannels, SONIC_MIN_CHANNELS, SONIC_MAX_CHANNELS);
  if (stream == NULL) {
    return NULL;
  }
  if (!allocateStreamBuffers(stream, sampleRate, numChannels)) {
    return NULL;
  }
  stream->speed = 1.0f;
  stream->pitch = 1.0f;
  stream->volume = 1.0f;
  stream->rate = 1.0f;
  stream->oldRatePosition = 0;
  stream->newRatePosition = 0;
  stream->quality = 0;
  return stream;
}


int sonicGetSampleRate(sonicStream stream) { return stream->sampleRate; }



void sonicSetSampleRate(sonicStream stream, int sampleRate) {
  sampleRate = CLAMP(sampleRate, SONIC_MIN_SAMPLE_RATE, SONIC_MAX_SAMPLE_RATE);
  freeStreamBuffers(stream);
  allocateStreamBuffers(stream, sampleRate, stream->numChannels);
}


int sonicGetNumChannels(sonicStream stream) { return stream->numChannels; }



void sonicSetNumChannels(sonicStream stream, int numChannels) {
  numChannels = CLAMP(numChannels, SONIC_MIN_CHANNELS, SONIC_MAX_CHANNELS);
  freeStreamBuffers(stream);
  allocateStreamBuffers(stream, stream->sampleRate, numChannels);
}


static int enlargeOutputBufferIfNeeded(sonicStream stream, int numSamples) {
  int outputBufferSize = stream->outputBufferSize;

  if (stream->numOutputSamples + numSamples > outputBufferSize) {
    stream->outputBufferSize += (outputBufferSize >> 1) + numSamples;
    stream->outputBuffer = (short*)sonicRealloc(
        stream->outputBuffer, outputBufferSize, stream->outputBufferSize,
        sizeof(short) * stream->numChannels);
    if (stream->outputBuffer == NULL) {
      return 0;
    }
  }
  return 1;
}


static int enlargeInputBufferIfNeeded(sonicStream stream, int numSamples) {
  int inputBufferSize = stream->inputBufferSize;

  if (stream->numInputSamples + numSamples > inputBufferSize) {
    stream->inputBufferSize += (inputBufferSize >> 1) + numSamples;
    stream->inputBuffer = (short*)sonicRealloc(
        stream->inputBuffer, inputBufferSize, stream->inputBufferSize,
        sizeof(short) * stream->numChannels);
    if (stream->inputBuffer == NULL) {
      return 0;
    }
  }
  return 1;
}




static void updateNumInputSamples(sonicStream stream, int numSamples) {
  float speed = stream->speed / stream->pitch;

  stream->numInputSamples += numSamples;
  stream->inputPlayTime += numSamples * stream->samplePeriod / speed;
}


static int addFloatSamplesToInputBuffer(sonicStream stream,
                                        const float* samples, int numSamples) {
  short* buffer;
  int count = numSamples * stream->numChannels;

  if (numSamples == 0) {
    return 1;
  }
  if (!enlargeInputBufferIfNeeded(stream, numSamples)) {
    return 0;
  }
  buffer = stream->inputBuffer + stream->numInputSamples * stream->numChannels;
  while (count--) {
    *buffer++ = (*samples++) * 32767.0f;
  }
  updateNumInputSamples(stream, numSamples);
  return 1;
}


static int addShortSamplesToInputBuffer(sonicStream stream,
                                        const short* samples, int numSamples) {
  if (numSamples == 0) {
    return 1;
  }
  if (!enlargeInputBufferIfNeeded(stream, numSamples)) {
    return 0;
  }
  memcpy(stream->inputBuffer + stream->numInputSamples * stream->numChannels,
         samples, numSamples * sizeof(short) * stream->numChannels);
  updateNumInputSamples(stream, numSamples);
  return 1;
}


static int addUnsignedCharSamplesToInputBuffer(sonicStream stream,
                                               const unsigned char* samples,
                                               int numSamples) {
  short* buffer;
  int count = numSamples * stream->numChannels;

  if (numSamples == 0) {
    return 1;
  }
  if (!enlargeInputBufferIfNeeded(stream, numSamples)) {
    return 0;
  }
  buffer = stream->inputBuffer + stream->numInputSamples * stream->numChannels;
  while (count--) {
    *buffer++ = (*samples++ - 128) << 8;
  }
  updateNumInputSamples(stream, numSamples);
  return 1;
}


static void removeInputSamples(sonicStream stream, int position) {
  int remainingSamples = stream->numInputSamples - position;

  if (remainingSamples > 0) {
    memmove(stream->inputBuffer,
            stream->inputBuffer + position * stream->numChannels,
            remainingSamples * sizeof(short) * stream->numChannels);
  }
  

  stream->inputPlayTime =
      (stream->inputPlayTime * remainingSamples) / stream->numInputSamples;
  stream->numInputSamples = remainingSamples;
}



static int copyInputToOutput(sonicStream stream, int numSamples) {
  if (!enlargeOutputBufferIfNeeded(stream, numSamples)) {
    return 0;
  }
  memcpy(stream->outputBuffer + stream->numOutputSamples * stream->numChannels,
         stream->inputBuffer, numSamples * sizeof(short) * stream->numChannels);
  stream->numOutputSamples += numSamples;
  removeInputSamples(stream, numSamples);
  return 1;
}


static int copyToOutput(sonicStream stream, short* samples, int numSamples) {
  if (!enlargeOutputBufferIfNeeded(stream, numSamples)) {
    return 0;
  }
  memcpy(stream->outputBuffer + stream->numOutputSamples * stream->numChannels,
         samples, numSamples * sizeof(short) * stream->numChannels);
  stream->numOutputSamples += numSamples;
  return 1;
}



int sonicReadFloatFromStream(sonicStream stream, float* samples,
                             int maxSamples) {
  int numSamples = stream->numOutputSamples;
  int remainingSamples = 0;
  short* buffer;
  int count;

  if (numSamples == 0) {
    return 0;
  }
  if (numSamples > maxSamples) {
    remainingSamples = numSamples - maxSamples;
    numSamples = maxSamples;
  }
  buffer = stream->outputBuffer;
  count = numSamples * stream->numChannels;
  while (count--) {
    *samples++ = (*buffer++) / 32767.0f;
  }
  if (remainingSamples > 0) {
    memmove(stream->outputBuffer,
            stream->outputBuffer + numSamples * stream->numChannels,
            remainingSamples * sizeof(short) * stream->numChannels);
  }
  stream->numOutputSamples = remainingSamples;
  return numSamples;
}



int sonicReadShortFromStream(sonicStream stream, short* samples,
                             int maxSamples) {
  int numSamples = stream->numOutputSamples;
  int remainingSamples = 0;

  if (numSamples == 0) {
    return 0;
  }
  if (numSamples > maxSamples) {
    remainingSamples = numSamples - maxSamples;
    numSamples = maxSamples;
  }
  memcpy(samples, stream->outputBuffer,
         numSamples * sizeof(short) * stream->numChannels);
  if (remainingSamples > 0) {
    memmove(stream->outputBuffer,
            stream->outputBuffer + numSamples * stream->numChannels,
            remainingSamples * sizeof(short) * stream->numChannels);
  }
  stream->numOutputSamples = remainingSamples;
  return numSamples;
}



int sonicReadUnsignedCharFromStream(sonicStream stream, unsigned char* samples,
                                    int maxSamples) {
  int numSamples = stream->numOutputSamples;
  int remainingSamples = 0;
  short* buffer;
  int count;

  if (numSamples == 0) {
    return 0;
  }
  if (numSamples > maxSamples) {
    remainingSamples = numSamples - maxSamples;
    numSamples = maxSamples;
  }
  buffer = stream->outputBuffer;
  count = numSamples * stream->numChannels;
  while (count--) {
    *samples++ = (char)((*buffer++) >> 8) + 128;
  }
  if (remainingSamples > 0) {
    memmove(stream->outputBuffer,
            stream->outputBuffer + numSamples * stream->numChannels,
            remainingSamples * sizeof(short) * stream->numChannels);
  }
  stream->numOutputSamples = remainingSamples;
  return numSamples;
}




int sonicFlushStream(sonicStream stream) {
  int maxRequired = stream->maxRequired;
  int remainingSamples = stream->numInputSamples;
  float speed = stream->speed / stream->pitch;
  float rate = stream->rate * stream->pitch;
  int expectedOutputSamples =
      stream->numOutputSamples +
      (int)((remainingSamples / speed + stream->numPitchSamples) / rate + 0.5f);

  
  if (!enlargeInputBufferIfNeeded(stream, remainingSamples + 2 * maxRequired)) {
    return 0;
  }
  memset(stream->inputBuffer + remainingSamples * stream->numChannels, 0,
         2 * maxRequired * sizeof(short) * stream->numChannels);
  stream->numInputSamples += 2 * maxRequired;
  if (!sonicWriteShortToStream(stream, NULL, 0)) {
    return 0;
  }
  
  if (stream->numOutputSamples > expectedOutputSamples) {
    stream->numOutputSamples = expectedOutputSamples;
  }
  
  stream->numInputSamples = 0;
  stream->inputPlayTime = 0.0f;
  stream->timeError = 0.0f;
  stream->numPitchSamples = 0;
  return 1;
}


int sonicSamplesAvailable(sonicStream stream) {
  return stream->numOutputSamples;
}




static void downSampleInput(sonicStream stream, short* samples, int skip) {
  int numSamples = stream->maxRequired / skip;
  int samplesPerValue = stream->numChannels * skip;
  int i, j;
  int value;
  short* downSamples = stream->downSampleBuffer;

  for (i = 0; i < numSamples; i++) {
    value = 0;
    for (j = 0; j < samplesPerValue; j++) {
      value += *samples++;
    }
    value /= samplesPerValue;
    *downSamples++ = value;
  }
}



static int findPitchPeriodInRange(short* samples, int minPeriod, int maxPeriod,
                                  int* retMinDiff, int* retMaxDiff) {
  int period, bestPeriod = 0, worstPeriod = 255;
  short* s;
  short* p;
  short sVal, pVal;
  unsigned long diff, minDiff = 1, maxDiff = 0;
  int i;

  for (period = minPeriod; period <= maxPeriod; period++) {
    diff = 0;
    s = samples;
    p = samples + period;
    for (i = 0; i < period; i++) {
      sVal = *s++;
      pVal = *p++;
      diff += sVal >= pVal ? (unsigned short)(sVal - pVal)
                           : (unsigned short)(pVal - sVal);
    }
    


    if (bestPeriod == 0 || diff * bestPeriod < minDiff * period) {
      minDiff = diff;
      bestPeriod = period;
    }
    if (diff * worstPeriod > maxDiff * period) {
      maxDiff = diff;
      worstPeriod = period;
    }
  }
  *retMinDiff = minDiff / bestPeriod;
  *retMaxDiff = maxDiff / worstPeriod;
  return bestPeriod;
}




static int prevPeriodBetter(sonicStream stream, int minDiff, int maxDiff,
                            int preferNewPeriod) {
  if (minDiff == 0 || stream->prevPeriod == 0) {
    return 0;
  }
  if (preferNewPeriod) {
    if (maxDiff > minDiff * 3) {
      
      return 0;
    }
    if (minDiff * 2 <= stream->prevMinDiff * 3) {
      
      return 0;
    }
  } else {
    if (minDiff <= stream->prevMinDiff) {
      return 0;
    }
  }
  return 1;
}






static int findPitchPeriod(sonicStream stream, short* samples,
                           int preferNewPeriod) {
  int minPeriod = stream->minPeriod;
  int maxPeriod = stream->maxPeriod;
  int minDiff, maxDiff, retPeriod;
  int skip = computeSkip(stream, stream->sampleRate);
  int period;

  if (stream->numChannels == 1 && skip == 1) {
    period = findPitchPeriodInRange(samples, minPeriod, maxPeriod, &minDiff,
                                    &maxDiff);
  } else {
    downSampleInput(stream, samples, skip);
    period = findPitchPeriodInRange(stream->downSampleBuffer, minPeriod / skip,
                                    maxPeriod / skip, &minDiff, &maxDiff);
    if (skip != 1) {
      period *= skip;
      minPeriod = period - (skip << 2);
      maxPeriod = period + (skip << 2);
      if (minPeriod < stream->minPeriod) {
        minPeriod = stream->minPeriod;
      }
      if (maxPeriod > stream->maxPeriod) {
        maxPeriod = stream->maxPeriod;
      }
      if (stream->numChannels == 1) {
        period = findPitchPeriodInRange(samples, minPeriod, maxPeriod, &minDiff,
                                        &maxDiff);
      } else {
        downSampleInput(stream, samples, 1);
        period = findPitchPeriodInRange(stream->downSampleBuffer, minPeriod,
                                        maxPeriod, &minDiff, &maxDiff);
      }
    }
  }
  if (prevPeriodBetter(stream, minDiff, maxDiff, preferNewPeriod)) {
    retPeriod = stream->prevPeriod;
  } else {
    retPeriod = period;
  }
  stream->prevMinDiff = minDiff;
  stream->prevPeriod = period;
  return retPeriod;
}



static void overlapAdd(int numSamples, int numChannels, short* out,
                       short* rampDown, short* rampUp) {
  short* o;
  short* u;
  short* d;
  int i, t;

  for (i = 0; i < numChannels; i++) {
    o = out + i;
    u = rampUp + i;
    d = rampDown + i;
    for (t = 0; t < numSamples; t++) {
#ifdef SONIC_USE_SIN
      float ratio = sin(t * M_PI / (2 * numSamples));
      *o = *d * (1.0f - ratio) + *u * ratio;
#else
      *o = (*d * (numSamples - t) + *u * t) / numSamples;
#endif
      o += numChannels;
      d += numChannels;
      u += numChannels;
    }
  }
}


static int moveNewSamplesToPitchBuffer(sonicStream stream,
                                       int originalNumOutputSamples) {
  int numSamples = stream->numOutputSamples - originalNumOutputSamples;
  int numChannels = stream->numChannels;
  int pitchBufferSize = stream->pitchBufferSize;

  if (stream->numPitchSamples + numSamples > pitchBufferSize) {
    stream->pitchBufferSize += (pitchBufferSize >> 1) + numSamples;
    stream->pitchBuffer = (short*)sonicRealloc(
        stream->pitchBuffer, pitchBufferSize, stream->pitchBufferSize,
        sizeof(short) * numChannels);
  }
  memcpy(stream->pitchBuffer + stream->numPitchSamples * numChannels,
         stream->outputBuffer + originalNumOutputSamples * numChannels,
         numSamples * sizeof(short) * numChannels);
  stream->numOutputSamples = originalNumOutputSamples;
  stream->numPitchSamples += numSamples;
  return 1;
}


static void removePitchSamples(sonicStream stream, int numSamples) {
  int numChannels = stream->numChannels;
  short* source = stream->pitchBuffer + numSamples * numChannels;

  if (numSamples == 0) {
    return;
  }
  if (numSamples != stream->numPitchSamples) {
    memmove(
        stream->pitchBuffer, source,
        (stream->numPitchSamples - numSamples) * sizeof(short) * numChannels);
  }
  stream->numPitchSamples -= numSamples;
}


static int findSincCoefficient(int i, int ratio, int width) {
  int lobePoints = (SINC_TABLE_SIZE - 1) / SINC_FILTER_POINTS;
  int left = i * lobePoints + (ratio * lobePoints) / width;
  int right = left + 1;
  int position = i * lobePoints * width + ratio * lobePoints - left * width;
  int leftVal = sincTable[left];
  int rightVal = sincTable[right];

  return ((leftVal * (width - position) + rightVal * position) << 1) / width;
}


static int getSign(int value) { return value >= 0 ? 1 : -1; }


static short interpolate(sonicStream stream, short* in, int oldSampleRate,
                         int newSampleRate) {
  
  int i;
  int total = 0;
  int position = stream->newRatePosition * oldSampleRate;
  int leftPosition = stream->oldRatePosition * newSampleRate;
  int rightPosition = (stream->oldRatePosition + 1) * newSampleRate;
  int ratio = rightPosition - position - 1;
  int width = rightPosition - leftPosition;
  int weight, value;
  int oldSign;
  int overflowCount = 0;

  for (i = 0; i < SINC_FILTER_POINTS; i++) {
    weight = findSincCoefficient(i, ratio, width);
    value = in[i * stream->numChannels] * weight;
    oldSign = getSign(total);
    total += value;
    if (oldSign != getSign(total) && getSign(value) == oldSign) {
      
      overflowCount += oldSign;
    }
  }
  
  if (overflowCount > 0) {
    return SHRT_MAX;
  } else if (overflowCount < 0) {
    return SHRT_MIN;
  }
  return total >> 16;
}


static int adjustRate(sonicStream stream, float rate,
                      int originalNumOutputSamples) {
  int newSampleRate = stream->sampleRate / rate;
  int oldSampleRate = stream->sampleRate;
  int numChannels = stream->numChannels;
  int position;
  short *in, *out;
  int i;
  int N = SINC_FILTER_POINTS;

  
  while (newSampleRate > (1 << 14) || oldSampleRate > (1 << 14)) {
    newSampleRate >>= 1;
    oldSampleRate >>= 1;
  }
  if (stream->numOutputSamples == originalNumOutputSamples) {
    return 1;
  }
  if (!moveNewSamplesToPitchBuffer(stream, originalNumOutputSamples)) {
    return 0;
  }
  
  for (position = 0; position < stream->numPitchSamples - N; position++) {
    while ((stream->oldRatePosition + 1) * newSampleRate >
           stream->newRatePosition * oldSampleRate) {
      if (!enlargeOutputBufferIfNeeded(stream, 1)) {
        return 0;
      }
      out = stream->outputBuffer + stream->numOutputSamples * numChannels;
      in = stream->pitchBuffer + position * numChannels;
      for (i = 0; i < numChannels; i++) {
        *out++ = interpolate(stream, in, oldSampleRate, newSampleRate);
        in++;
      }
      stream->newRatePosition++;
      stream->numOutputSamples++;
    }
    stream->oldRatePosition++;
    if (stream->oldRatePosition == oldSampleRate) {
      stream->oldRatePosition = 0;
      stream->newRatePosition = 0;
    }
  }
  removePitchSamples(stream, position);
  return 1;
}


static int skipPitchPeriod(sonicStream stream, short* samples, float speed,
                           int period) {
  long newSamples;
  int numChannels = stream->numChannels;

  if (speed >= 2.0f) {
    

    newSamples = period / (speed - 1.0f);
  } else {
    newSamples = period;
  }
  if (!enlargeOutputBufferIfNeeded(stream, newSamples)) {
    return 0;
  }
  overlapAdd(newSamples, numChannels,
             stream->outputBuffer + stream->numOutputSamples * numChannels,
             samples, samples + period * numChannels);
  stream->numOutputSamples += newSamples;
  return newSamples;
}


static int insertPitchPeriod(sonicStream stream, short* samples, float speed,
                             int period) {
  long newSamples;
  short* out;
  int numChannels = stream->numChannels;

  if (speed <= 0.5f) {
    newSamples = period * speed / (1.0f - speed);
  } else {
    newSamples = period;
  }
  if (!enlargeOutputBufferIfNeeded(stream, period + newSamples)) {
    return 0;
  }
  out = stream->outputBuffer + stream->numOutputSamples * numChannels;
  memcpy(out, samples, period * sizeof(short) * numChannels);
  out =
      stream->outputBuffer + (stream->numOutputSamples + period) * numChannels;
  overlapAdd(newSamples, numChannels, out, samples + period * numChannels,
             samples);
  stream->numOutputSamples += period + newSamples;
  return newSamples;
}



static int copyUnmodifiedSamples(sonicStream stream, short* samples,
                                 float speed, int position, int* newSamples) {
  int availableSamples = stream->numInputSamples - position;
  float inputToCopyFloat =
      1 - stream->timeError * speed / (stream->samplePeriod * (speed - 1.0));

  *newSamples = inputToCopyFloat > availableSamples ? availableSamples
                                                    : (int)inputToCopyFloat;
  if (!copyToOutput(stream, samples, *newSamples)) {
    return 0;
  }
  stream->timeError +=
      *newSamples * stream->samplePeriod * (speed - 1.0) / speed;
  return 1;
}



static int changeSpeed(sonicStream stream, float speed) {
  short* samples;
  int numSamples = stream->numInputSamples;
  int position = 0, period, newSamples;
  int maxRequired = stream->maxRequired;

  if (stream->numInputSamples < maxRequired) {
    return 1;
  }
  do {
    samples = stream->inputBuffer + position * stream->numChannels;
    if ((speed > 1.0f && speed < 2.0f && stream->timeError < 0.0f) ||
        (speed < 1.0f && speed > 0.5f && stream->timeError > 0.0f)) {
      

      if (!copyUnmodifiedSamples(stream, samples, speed, position,
                                 &newSamples)) {
        return 0;
      }
      position += newSamples;
    } else {
      

      period = findPitchPeriod(stream, samples, 1);
#ifdef SONIC_SPECTROGRAM
      if (stream->spectrogram != NULL) {
        sonicAddPitchPeriodToSpectrogram(stream->spectrogram, samples, period,
                                         stream->numChannels);
        newSamples = period;
        position += period;
      } else
#endif 
        if (speed > 1.0) {
          newSamples = skipPitchPeriod(stream, samples, speed, period);
          position += period + newSamples;
          if (speed < 2.0) {
            stream->timeError += newSamples * stream->samplePeriod -
                                 (period + newSamples) * stream->inputPlayTime /
                                     stream->numInputSamples;
          }
        } else {
          newSamples = insertPitchPeriod(stream, samples, speed, period);
          position += newSamples;
          if (speed > 0.5) {
            stream->timeError +=
                (period + newSamples) * stream->samplePeriod -
                newSamples * stream->inputPlayTime / stream->numInputSamples;
          }
        }
      if (newSamples == 0) {
        return 0; 
      }
    }
  } while (position + maxRequired <= numSamples);
  removeInputSamples(stream, position);
  return 1;
}




static int processStreamInput(sonicStream stream) {
  int originalNumOutputSamples = stream->numOutputSamples;
  float rate = stream->rate * stream->pitch;
  float localSpeed;

  if (stream->numInputSamples == 0) {
    return 1;
  }
  localSpeed =
      stream->numInputSamples * stream->samplePeriod / stream->inputPlayTime;
  if (localSpeed > 1.00001 || localSpeed < 0.99999) {
    changeSpeed(stream, localSpeed);
  } else {
    if (!copyInputToOutput(stream, stream->numInputSamples)) {
      return 0;
    }
  }
  if (rate != 1.0f) {
    if (!adjustRate(stream, rate, originalNumOutputSamples)) {
      return 0;
    }
  }
  if (stream->volume != 1.0f) {
    
    scaleSamples(
        stream->outputBuffer + originalNumOutputSamples * stream->numChannels,
        (stream->numOutputSamples - originalNumOutputSamples) *
            stream->numChannels,
        stream->volume);
  }
  return 1;
}


int sonicWriteFloatToStream(sonicStream stream, const float* samples,
                            int numSamples) {
  if (!addFloatSamplesToInputBuffer(stream, samples, numSamples)) {
    return 0;
  }
  return processStreamInput(stream);
}



int sonicWriteShortToStream(sonicStream stream, const short* samples,
                            int numSamples) {
  if (!addShortSamplesToInputBuffer(stream, samples, numSamples)) {
    return 0;
  }
  return processStreamInput(stream);
}



int sonicWriteUnsignedCharToStream(sonicStream stream,
                                   const unsigned char* samples,
                                   int numSamples) {
  if (!addUnsignedCharSamplesToInputBuffer(stream, samples, numSamples)) {
    return 0;
  }
  return processStreamInput(stream);
}



int sonicChangeFloatSpeed(float* samples, int numSamples, float speed,
                          float pitch, float rate, float volume,
                          int useChordPitch, int sampleRate, int numChannels) {
  sonicStream stream = sonicCreateStream(sampleRate, numChannels);

  sonicSetSpeed(stream, speed);
  sonicSetPitch(stream, pitch);
  sonicSetRate(stream, rate);
  sonicSetVolume(stream, volume);
  sonicWriteFloatToStream(stream, samples, numSamples);
  sonicFlushStream(stream);
  numSamples = sonicSamplesAvailable(stream);
  sonicReadFloatFromStream(stream, samples, numSamples);
  sonicDestroyStream(stream);
  return numSamples;
}



int sonicChangeShortSpeed(short* samples, int numSamples, float speed,
                          float pitch, float rate, float volume,
                          int useChordPitch, int sampleRate, int numChannels) {
  sonicStream stream = sonicCreateStream(sampleRate, numChannels);

  sonicSetSpeed(stream, speed);
  sonicSetPitch(stream, pitch);
  sonicSetRate(stream, rate);
  sonicSetVolume(stream, volume);
  sonicWriteShortToStream(stream, samples, numSamples);
  sonicFlushStream(stream);
  numSamples = sonicSamplesAvailable(stream);
  sonicReadShortFromStream(stream, samples, numSamples);
  sonicDestroyStream(stream);
  return numSamples;
}
