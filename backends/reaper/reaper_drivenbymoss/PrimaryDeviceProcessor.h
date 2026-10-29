// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

#ifndef _DBM_PRIMARYDEVICEPROCESSOR_H_
#define _DBM_PRIMARYDEVICEPROCESSOR_H_

#include "DeviceProcessor.h"
#include "ReaperUtils.h"

/**
 * Processes the commands of the first instrument of the selected track ("primary", protocol v2),
 * e.g. its parameters, which used to go to the selected device. Reaper has no layers, the
 * commands of drum pads and layers are ignored.
 */
class PrimaryDeviceProcessor : public DeviceProcessor
{
public:
	explicit PrimaryDeviceProcessor(Model& aModel) : DeviceProcessor(aModel) {};

protected:
	int GetDeviceSelection() noexcept override
	{
		MediaTrack* track = GetSelectedTrack2(ReaperUtils::GetProject(), 0, true);
		return track == nullptr ? -1 : TrackFX_GetInstrument(track);
	}
};

#endif /* _DBM_PRIMARYDEVICEPROCESSOR_H_ */
