#ifndef SONIC_H_
#define SONIC_H_

/* Sonic library
   Copyright 2010
   Bill Cox
   This file is part of the Sonic Library.

   This file is licensed under the Apache 2.0 license.
*/













#ifdef __cplusplus
extern "C" {
#endif

#ifdef SONIC_INTERNAL













#define sonicCreateStream sonicIntCreateStream
#define sonicDestroyStream sonicIntDestroyStream
#define sonicWriteFloatToStream sonicIntWriteFloatToStream
#define sonicWriteShortToStream sonicIntWriteShortToStream
#define sonicWriteUnsignedCharToStream sonicIntWriteUnsignedCharToStream
#define sonicReadFloatFromStream sonicIntReadFloatFromStream
#define sonicReadShortFromStream sonicIntReadShortFromStream
#define sonicReadUnsignedCharFromStream sonicIntReadUnsignedCharFromStream
#define sonicFlushStream sonicIntFlushStream
#define sonicSamplesAvailable sonicIntSamplesAvailable
#define sonicGetSpeed sonicIntGetSpeed
#define sonicSetSpeed sonicIntSetSpeed
#define sonicGetPitch sonicIntGetPitch
#define sonicSetPitch sonicIntSetPitch
#define sonicGetRate sonicIntGetRate
#define sonicSetRate sonicIntSetRate
#define sonicGetVolume sonicIntGetVolume
#define sonicSetVolume sonicIntSetVolume
#define sonicGetQuality sonicIntGetQuality
#define sonicSetQuality sonicIntSetQuality
#define sonicGetSampleRate sonicIntGetSampleRate
#define sonicSetSampleRate sonicIntSetSampleRate
#define sonicGetNumChannels sonicIntGetNumChannels
#define sonicGetUserData sonicIntGetUserData
#define sonicSetUserData sonicIntSetUserData
#define sonicSetNumChannels sonicIntSetNumChannels
#define sonicChangeFloatSpeed sonicIntChangeFloatSpeed
#define sonicChangeShortSpeed sonicIntChangeShortSpeed
#define sonicEnableNonlinearSpeedup sonicIntEnableNonlinearSpeedup
#define sonicSetDurationFeedbackStrength sonicIntSetDurationFeedbackStrength
#define sonicComputeSpectrogram sonicIntComputeSpectrogram
#define sonicGetSpectrogram sonicIntGetSpectrogram

#endif 



#ifndef SONIC_MIN_PITCH
#define SONIC_MIN_PITCH 65
#endif  
#ifndef SONIC_MAX_PITCH
#define SONIC_MAX_PITCH 400
#endif  



#define SONIC_MIN_VOLUME 0.01f
#define SONIC_MAX_VOLUME 100.0f
#define SONIC_MIN_SPEED 0.05f
#define SONIC_MAX_SPEED 20.0f
#define SONIC_MIN_PITCH_SETTING 0.05f
#define SONIC_MAX_PITCH_SETTING 20.0f
#define SONIC_MIN_RATE 0.05f
#define SONIC_MAX_RATE 20.0f
#define SONIC_MIN_SAMPLE_RATE 1000
#define SONIC_MAX_SAMPLE_RATE 500000
#define SONIC_MIN_CHANNELS 1
#define SONIC_MAX_CHANNELS 32


#define SONIC_AMDF_FREQ 4000

struct sonicStreamStruct;
typedef struct sonicStreamStruct* sonicStream;






sonicStream sonicCreateStream(int sampleRate, int numChannels);

void sonicDestroyStream(sonicStream stream);

void sonicSetUserData(sonicStream stream, void *userData);

void *sonicGetUserData(sonicStream stream);



int sonicWriteFloatToStream(sonicStream stream, const float* samples, int numSamples);


int sonicWriteShortToStream(sonicStream stream, const short* samples, int numSamples);


int sonicWriteUnsignedCharToStream(sonicStream stream, const unsigned char* samples,
                                   int numSamples);


int sonicReadFloatFromStream(sonicStream stream, float* samples,
                             int maxSamples);


int sonicReadShortFromStream(sonicStream stream, short* samples,
                             int maxSamples);


int sonicReadUnsignedCharFromStream(sonicStream stream, unsigned char* samples,
                                    int maxSamples);



int sonicFlushStream(sonicStream stream);

int sonicSamplesAvailable(sonicStream stream);

float sonicGetSpeed(sonicStream stream);

void sonicSetSpeed(sonicStream stream, float speed);

float sonicGetPitch(sonicStream stream);

void sonicSetPitch(sonicStream stream, float pitch);

float sonicGetRate(sonicStream stream);

void sonicSetRate(sonicStream stream, float rate);

float sonicGetVolume(sonicStream stream);

void sonicSetVolume(sonicStream stream, float volume);



int sonicGetChordPitch(sonicStream stream);


void sonicSetChordPitch(sonicStream stream, int useChordPitch);

int sonicGetQuality(sonicStream stream);


void sonicSetQuality(sonicStream stream, int quality);

int sonicGetSampleRate(sonicStream stream);


void sonicSetSampleRate(sonicStream stream, int sampleRate);

int sonicGetNumChannels(sonicStream stream);


void sonicSetNumChannels(sonicStream stream, int numChannels);




int sonicChangeFloatSpeed(float* samples, int numSamples, float speed,
                          float pitch, float rate, float volume,
                          int useChordPitch, int sampleRate, int numChannels);




int sonicChangeShortSpeed(short* samples, int numSamples, float speed,
                          float pitch, float rate, float volume,
                          int useChordPitch, int sampleRate, int numChannels);

#ifdef SONIC_SPECTROGRAM


















#define SONIC_MAX_SPECTRUM_FREQ 5000

struct sonicSpectrogramStruct;
struct sonicBitmapStruct;
typedef struct sonicSpectrogramStruct* sonicSpectrogram;
typedef struct sonicBitmapStruct* sonicBitmap;




struct sonicBitmapStruct {
  unsigned char* data;
  int numRows;
  int numCols;
};


void sonicComputeSpectrogram(sonicStream stream);


sonicSpectrogram sonicGetSpectrogram(sonicStream stream);



sonicSpectrogram sonicCreateSpectrogram(int sampleRate);



void sonicDestroySpectrogram(sonicSpectrogram spectrogram);


sonicBitmap sonicConvertSpectrogramToBitmap(sonicSpectrogram spectrogram,
                                            int numRows, int numCols);


void sonicDestroyBitmap(sonicBitmap bitmap);

int sonicWritePGM(sonicBitmap bitmap, char* fileName);




void sonicAddPitchPeriodToSpectrogram(sonicSpectrogram spectrogram,
                                      short* samples, int numSamples,
                                      int numChannels);
#endif  

#ifdef __cplusplus
}
#endif

#endif  
