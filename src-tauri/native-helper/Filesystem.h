#import <Foundation/Foundation.h>
#include <sys/types.h>

// No subprocesses. All validation and source reads finish before mutation.
NSDictionary *PXApply(NSDictionary *payload, uid_t uid);
#ifdef PX_TESTING
void PXTestFailAfterPublish(NSInteger count);
BOOL PXTestOwnerAllowed(uid_t owner, uid_t peer, BOOL existingTree);
// Deterministic scheduling hooks, never compiled into the production helper.
void PXTestSetCleanupHook(void (^hook)(NSString *event, int parent, NSString *name));
NSDictionary *PXTestRemove(int parent, NSString *name, int stage, uid_t peer);
// Linked only by the standalone selftest, never selected by a payload field.
NSDictionary *PXTestApply(NSDictionary *payload, uid_t uid, NSString *home, NSString *applications);
#endif
