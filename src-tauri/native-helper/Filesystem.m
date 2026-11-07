#import "Filesystem.h"
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <dirent.h>
#include <pwd.h>
#include <errno.h>
#include <limits.h>
#include <sys/acl.h>
#include <membership.h>

// Deliberately conservative: bounded in-memory snapshots avoid reopening mutable
// client paths after authorization. No source descriptor is used for writes.
static const NSUInteger PXMaxBytes = 128 * 1024 * 1024;
static const NSUInteger PXMaxNodes = 20000;
@interface PXFD : NSObject
@property(nonatomic) int fd;
+ (instancetype)take:(int)fd;
@end
@implementation PXFD
+ (instancetype)take:(int)fd { if (fd < 0) @throw [NSException exceptionWithName:@"PXFilesystem" reason:@"Cannot safely open filesystem object" userInfo:nil]; PXFD *v = [PXFD new]; v.fd = fd; return v; }
- (void)dealloc { close(_fd); }
@end
static void Fail(NSString *s) { @throw [NSException exceptionWithName:@"PXFilesystem" reason:s userInfo:nil]; }
static void Check(BOOL b, NSString *s) { if (!b) Fail(s); }
static NSArray *Parts(id value) {
    Check([value isKindOfClass:NSString.class], @"Path must be a string");
    NSString *p = value;
    Check(p.length > 1 && [p hasPrefix:@"/"] && [p lengthOfBytesUsingEncoding:NSUTF8StringEncoding] < PATH_MAX, @"Invalid absolute path");
    Check([p rangeOfCharacterFromSet:NSCharacterSet.controlCharacterSet].location == NSNotFound, @"Control character in path");
    NSArray *a = [[p substringFromIndex:1] componentsSeparatedByString:@"/"];
    for (NSString *s in a) Check(s.length && ![s isEqual:@"."] && ![s isEqual:@".."] && [s lengthOfBytesUsingEncoding:NSUTF8StringEncoding] <= NAME_MAX, @"Invalid path component");
    return a;
}
static struct stat Stat(int fd) { struct stat s; Check(fstat(fd, &s) == 0, @"Cannot stat object"); return s; }
static PXFD *Dir(int parent, NSString *name) { return [PXFD take:openat(parent, name.fileSystemRepresentation, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)]; }
static PXFD *AbsoluteDir(NSString *path) {
    PXFD *d = [PXFD take:open("/", O_RDONLY | O_DIRECTORY | O_CLOEXEC)];
    for (NSString *s in Parts(path)) d = Dir(d.fd, s);
    return d;
}
// Keep the production ownership boundary independently testable: test fixtures
// may use their own uid for root-owned ancestors, but policy assertions must not.
static BOOL OwnerAllowed(uid_t owner, uid_t peer, BOOL existingTree) {
    return owner == 0 || (existingTree && owner == peer);
}
static NSString *FDPath(int fd) {
    char path[PATH_MAX] = {0};
    if (fcntl(fd, F_GETPATH, path) != -1 && memchr(path, 0, sizeof(path))) {
        NSString *value = [[NSString alloc] initWithUTF8String:path];
        if (value) return value;
    }
    return @"<path unavailable>";
}
static void TrustedObject(int fd, uid_t peer, BOOL existingTree) {
    struct stat s = Stat(fd);
    BOOL allowed = OwnerAllowed(s.st_uid, peer, existingTree);
#ifdef PX_TESTING
    allowed = allowed || s.st_uid == getuid();
#endif
    Check(allowed, [NSString stringWithFormat:@"Untrusted destination owner at %@ (uid %u). %@. Move the output to a supported root-owned installation; do not change ancestor ownership automatically.",
        FDPath(fd), (unsigned)s.st_uid, existingTree
        ? [NSString stringWithFormat:@"Selected output must belong to root (uid 0) or the authenticated requesting user (uid %u)", (unsigned)peer]
        : @"Installation roots and destination ancestors must belong to root (uid 0)"]);
    Check(!(s.st_mode & 0002) && (!(s.st_mode & 0020) || s.st_gid == 80), @"Untrusted writable destination ancestor");
    acl_t acl=acl_get_fd_np(fd,ACL_TYPE_EXTENDED);
    if (!acl) { Check(errno==ENOENT || errno==ENOTSUP, @"Cannot inspect destination ACL"); return; }
    @try {
        acl_entry_t entry; int status=acl_get_entry(acl,ACL_FIRST_ENTRY,&entry);
        while (status==0) {
            acl_tag_t tag; Check(acl_get_tag_type(entry,&tag)==0,@"Cannot inspect ACL tag");
            if (tag==ACL_EXTENDED_ALLOW) {
                acl_permset_t perms; Check(acl_get_permset(entry,&perms)==0,@"Cannot inspect ACL rights");
                BOOL writes=NO;
                acl_perm_t rights[]={ACL_WRITE_DATA,ACL_APPEND_DATA,ACL_DELETE,ACL_DELETE_CHILD,ACL_WRITE_ATTRIBUTES,ACL_WRITE_EXTATTRIBUTES,ACL_WRITE_SECURITY,ACL_CHANGE_OWNER};
                for (NSUInteger i=0;i<sizeof(rights)/sizeof(rights[0]);i++) { int bit=acl_get_perm_np(perms,rights[i]); Check(bit>=0,@"Cannot inspect ACL permission"); if(bit) writes=YES; }
                if (writes) {
                    void *q=acl_get_qualifier(entry); Check(q!=NULL,@"Cannot inspect ACL identity");
                    id_t identity=0; int type=0; int result=mbr_uuid_to_id(q,&identity,&type); acl_free(q);
                    Check(result==0 && ((type==ID_TYPE_UID && identity==0) || (type==ID_TYPE_GID && identity==80)),@"Untrusted ACL write grant");
                }
            }
            status=acl_get_entry(acl,ACL_NEXT_ENTRY,&entry);
        }
        Check(errno==EINVAL || status==1,@"Cannot enumerate ACL");
    } @finally { acl_free(acl); }
}
// Only explicitly selected output trees may be owned by the authenticated peer.
// Installation roots and all ancestors still require root ownership.
static void Trusted(int fd) { TrustedObject(fd, 0, NO); }
static NSArray *Names(int fd) {
    int copy = openat(fd, ".", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    Check(copy >= 0, @"Cannot enumerate directory");
    DIR *dir = fdopendir(copy); if (!dir) { close(copy); Fail(@"Cannot enumerate directory"); }
    NSMutableArray *a = [NSMutableArray new];
    @try {
        errno = 0; struct dirent *e;
        while ((e = readdir(dir))) {
            if (!strcmp(e->d_name,".") || !strcmp(e->d_name,"..")) continue;
            NSString *n = [[NSString alloc] initWithBytes:e->d_name length:strlen(e->d_name) encoding:NSUTF8StringEncoding];
            // Finder's custom folder icon is literally named "Icon\r". These
            // are kernel-supplied single components, used only with *at APIs
            // (never interpolated into a command). Retain valid UTF-8 names,
            // including this metadata; API-supplied paths remain strict.
            Check(n != nil, @"Directory entry is not valid UTF-8");
            Check(a.count < PXMaxNodes, @"Too many directory entries"); [a addObject:n]; errno = 0;
        }
        Check(errno == 0, @"Cannot read directory");
    } @finally { closedir(dir); }
    return [a sortedArrayUsingSelector:@selector(compare:)];
}
static NSDictionary *Snapshot(int parent, NSString *name, uid_t uid, dev_t device, NSUInteger depth, NSUInteger *nodes, NSUInteger *bytes, BOOL source) {
    Check(depth <= 64 && ++*nodes <= PXMaxNodes, @"Filesystem tree exceeds limits");
    PXFD *f = [PXFD take:openat(parent, name.fileSystemRepresentation, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)];
    struct stat s = Stat(f.fd);
    Check(s.st_dev == device && (S_ISDIR(s.st_mode) || S_ISREG(s.st_mode)), @"Symlink, mount or special file rejected");
    if (source) Check(s.st_uid == uid, @"Source must belong to peer uid");
    if (S_ISDIR(s.st_mode)) {
        if (!source) TrustedObject(f.fd,uid,YES);
        NSMutableDictionary *children = [NSMutableDictionary new];
        for (NSString *n in Names(f.fd)) children[n] = Snapshot(f.fd,n,uid,device,depth+1,nodes,bytes,source);
        return @{ @"children":children, @"mode":@(s.st_mode & 0777) };
    }
    Check(s.st_nlink == 1, @"Hardlinked files rejected");
    if (!source) return @{};
    Check(s.st_size >= 0 && (uint64_t)s.st_size <= PXMaxBytes - *bytes, @"Source size exceeds limit");
    NSMutableData *data = [NSMutableData new]; char buf[65536]; ssize_t n;
    while ((n = read(f.fd,buf,sizeof(buf))) != 0) {
        if (n < 0 && errno == EINTR) continue;
        Check(n > 0, @"Cannot read source"); Check((NSUInteger)n <= PXMaxBytes - *bytes, @"Source size exceeds limit");
        *bytes += n; [data appendBytes:buf length:(NSUInteger)n];
    }
    struct stat end = Stat(f.fd);
    Check(s.st_ino == end.st_ino && s.st_size == end.st_size && s.st_mtimespec.tv_sec == end.st_mtimespec.tv_sec && s.st_mtimespec.tv_nsec == end.st_mtimespec.tv_nsec && end.st_uid == uid && end.st_nlink == 1, @"Source changed during preflight");
    return @{ @"data":data, @"mode":@(s.st_mode & 0777) };
}
static NSArray *Destination(id path, NSString *applications) {
    Parts(path);
    NSString *prefix = [applications stringByAppendingString:@"/Adobe After Effects "];
    Check([path hasPrefix:prefix], @"Destination is outside standard After Effects folders");
    NSString *rest = [path substringFromIndex:applications.length+1];
    NSArray *a = [rest componentsSeparatedByString:@"/"];
    Check(a.count >= 3 && [a[0] length] > [@"Adobe After Effects " length] && ([a[1] isEqual:@"Scripts"] || [a[1] isEqual:@"Plug-ins"]), @"Invalid After Effects destination");
    Check(!(a.count == 3 && [a[1] isEqual:@"Scripts"] && [a[2] caseInsensitiveCompare:@"ScriptUI Panels"] == NSOrderedSame), @"ScriptUI Panels root is protected");
    for (NSString *n in a) Check(![n hasPrefix:@"."] && [n rangeOfString:@":"].location == NSNotFound, @"Hidden metadata paths and colon aliases are protected");
    return a;
}
static void VerifyAdobe(int fd) {
    for (NSString *name in Names(fd)) {
        if (![name hasSuffix:@".app"]) continue;
        @try {
            PXFD *app=Dir(fd,name); Trusted(app.fd);
            PXFD *contents=Dir(app.fd,@"Contents"); Trusted(contents.fd);
            PXFD *info=[PXFD take:openat(contents.fd,"Info.plist",O_RDONLY|O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC)];
            struct stat st=Stat(info.fd); Check(S_ISREG(st.st_mode) && st.st_size>0 && st.st_size<=1024*1024,@"Invalid Adobe Info.plist"); Trusted(info.fd);
            NSMutableData *data=[NSMutableData dataWithLength:(NSUInteger)st.st_size]; NSUInteger offset=0;
            while(offset<data.length) { ssize_t n=read(info.fd,(char *)data.mutableBytes+offset,data.length-offset); if(n<0 && errno==EINTR) continue; Check(n>0,@"Cannot read Adobe identity"); offset+=n; }
            id plist=[NSPropertyListSerialization propertyListWithData:data options:NSPropertyListImmutable format:NULL error:NULL];
            if ([plist isKindOfClass:NSDictionary.class] &&
                [@[@"com.adobe.AfterEffects", @"com.adobe.AfterEffects.application"] containsObject:plist[@"CFBundleIdentifier"] ?: @""]) return;
        } @catch(NSException *ignored) { (void)ignored; }
    }
    Fail(@"Destination requires an installed Adobe After Effects app with a trusted Info.plist");
}
static PXFD *Parent(PXFD *base, NSArray *a, BOOL create) {
    PXFD *d = base; Trusted(d.fd);
    for (NSUInteger i=0; i+1<a.count; i++) {
        struct stat s;
        if (fstatat(d.fd,[a[i] fileSystemRepresentation],&s,AT_SYMLINK_NOFOLLOW) != 0) {
            Check(errno == ENOENT, @"Cannot inspect destination ancestor");
            Check(i != 0,@"After Effects installation does not exist");
            if (!create) return nil;
            Check(mkdirat(d.fd,[a[i] fileSystemRepresentation],0755) == 0, @"Cannot create destination directory");
        }
        d = Dir(d.fd,a[i]); Trusted(d.fd); if(i==0) VerifyAdobe(d.fd); Check(Stat(d.fd).st_dev == Stat(base.fd).st_dev, @"Destination mount crossing rejected");
    }
    return d;
}
#ifdef PX_TESTING
static void (^PXCleanupHook)(NSString *, int, NSString *);
void PXTestSetCleanupHook(void (^hook)(NSString *, int, NSString *)) { PXCleanupHook = [hook copy]; }
BOOL PXTestOwnerAllowed(uid_t owner, uid_t peer, BOOL existingTree) { return OwnerAllowed(owner, peer, existingTree); }
static void CleanupHook(NSString *event, int parent, NSString *name) { if (PXCleanupHook) PXCleanupHook(event, parent, name); }
#else
static void CleanupHook(NSString *event, int parent, NSString *name) { (void)event; (void)parent; (void)name; }
#endif
// Only call on a newly created helper directory or an object already renamed
// into that directory. Never change ownership/ACLs through a client-tree fd:
// its owner could have moved the directory elsewhere since it was opened.
static void SealPrivateDirectory(int fd) {
    uid_t owner = 0;
#ifdef PX_TESTING
    owner = geteuid();
#endif
    Check(fchown(fd, owner, (gid_t)-1) == 0, @"Cannot take ownership of cleanup quarantine");
    acl_t empty = acl_init(0);
    Check(empty != NULL, @"Cannot allocate private cleanup ACL");
    int result = acl_set_fd_np(fd, empty, ACL_TYPE_EXTENDED);
    acl_free(empty);
    Check(result == 0 && fchmod(fd,0700) == 0, @"Cannot revoke access to cleanup quarantine");
    acl_t acl = acl_get_fd_np(fd, ACL_TYPE_EXTENDED);
    if (acl) {
        acl_entry_t entry;
        int status = acl_get_entry(acl, ACL_FIRST_ENTRY, &entry);
        int error = errno;
        acl_free(acl);
        Check(status == 1 || (status == -1 && error == EINVAL), @"Cleanup quarantine still has ACL entries");
    } else Check(errno == ENOENT || errno == ENOTSUP, @"Cannot verify cleanup ACL");
    // Check ownership LAST: a peer with ACL_CHANGE_OWNER could have raced the
    // initial chown before ACL revocation. Do not traverse if it regained owner.
    struct stat s = Stat(fd);
    Check(S_ISDIR(s.st_mode) && s.st_uid == owner && (s.st_mode & 07777) == 0700, @"Cleanup quarantine is not private");
}
// Persist intent before every rename. If cleanup stops, manifest old-N entries
// can be followed through this journal, including each captured descendant.
static void JournalCapture(int parent, NSString *name, int stage, NSString *slot) {
    NSString *path = FDPath(parent);
    Check(![path isEqual:@"<path unavailable>"], @"Cannot identify cleanup source for recovery");
    NSData *record = [NSJSONSerialization dataWithJSONObject:@{@"sourceParent":path,@"sourceName":name,@"quarantine":slot} options:0 error:NULL];
    Check(record != nil, @"Cannot encode cleanup recovery record");
    PXFD *log = [PXFD take:openat(stage,"cleanup-journal.jsonl",O_WRONLY|O_CREAT|O_APPEND|O_NOFOLLOW|O_CLOEXEC,0600)];
    struct stat st = Stat(log.fd);
    Check(S_ISREG(st.st_mode) && st.st_nlink == 1 && st.st_uid == geteuid(), @"Unsafe cleanup recovery journal");
    NSMutableData *line = [record mutableCopy]; [line appendBytes:"\n" length:1];
    const char *p = line.bytes; NSUInteger left = line.length;
    while (left) { ssize_t n = write(log.fd,p,left); if(n<0 && errno==EINTR) continue; Check(n>0,@"Cannot write cleanup recovery record"); p+=n; left-=n; }
    Check(fsync(log.fd)==0 && fsync(stage)==0, @"Cannot persist cleanup recovery record");
}
static void RemoveCaptured(int stage, NSString *slot, NSUInteger depth, uid_t peer, NSUInteger *nodes, BOOL committed);
static void CaptureAndRemove(int parent, NSString *name, int stage, NSUInteger depth, uid_t peer, NSUInteger *nodes, BOOL committed) {
    Check(depth <= 64 && ++*nodes <= PXMaxNodes, @"Cleanup tree exceeds limits");
    CleanupHook(@"before-capture", parent, name);
    NSString *slot = [@".cleanup-" stringByAppendingString:NSUUID.UUID.UUIDString];
    // The destination is private to the helper. Capture the name BEFORE opening
    // it: if a peer moved it away first, never follow a previously opened fd.
    JournalCapture(parent,name,stage,slot);
    if (renameat(parent,name.fileSystemRepresentation,stage,slot.fileSystemRepresentation) != 0) {
        Check(errno == ENOENT, @"Cannot quarantine removal (mount crossing or concurrent change)");
        return;
    }
    Check(fsync(stage) == 0 && fsync(parent) == 0, @"Cannot persist cleanup quarantine");
    CleanupHook(@"captured", stage, slot);
    RemoveCaptured(stage,slot,depth,peer,nodes,committed);
}
static void RemoveCaptured(int stage, NSString *slot, NSUInteger depth, uid_t peer, NSUInteger *nodes, BOOL committed) {
    // A retained fd to this object does not grant access to its new parent.
    // Even before sealing, a peer cannot rename the captured directory itself.
    PXFD *f = [PXFD take:openat(stage,slot.fileSystemRepresentation,O_RDONLY|O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC)];
    struct stat s = Stat(f.fd);
    Check(s.st_dev == Stat(stage).st_dev && (S_ISDIR(s.st_mode) || S_ISREG(s.st_mode)), @"Unsafe quarantined removal: symlink, mount or special file");
    if (S_ISDIR(s.st_mode)) {
        TrustedObject(f.fd,peer,committed);
        // Rollback only discards helper-created/root-owned published output.
        // Never seal a saved original while rollback may still need to restore it.
        if (committed) SealPrivateDirectory(f.fd);
        CleanupHook(@"sealed", f.fd, slot);
        for (NSString *n in Names(f.fd)) CaptureAndRemove(f.fd,n,stage,depth+1,peer,nodes,committed);
        Check(unlinkat(stage,slot.fileSystemRepresentation,AT_REMOVEDIR) == 0, @"Cannot remove quarantined directory");
    } else {
        Check(s.st_nlink == 1, @"Hardlinked cleanup file rejected");
        Check(unlinkat(stage,slot.fileSystemRepresentation,0) == 0, @"Cannot remove quarantined file");
    }
}
static void Remove(int parent, NSString *name, int stage, uid_t peer, BOOL committed) {
    NSUInteger nodes = 0;
    CaptureAndRemove(parent,name,stage,0,peer,&nodes,committed);
}
#ifdef PX_TESTING
NSDictionary *PXTestRemove(int parent, NSString *name, int stage, uid_t peer) {
    @try {
        SealPrivateDirectory(stage);
        Remove(parent,name,stage,peer,YES);
        return @{@"ok":@YES};
    } @catch(NSException *e) { return @{@"ok":@NO,@"error":e.reason}; }
}
#endif
static void Write(int parent, NSString *name, NSDictionary *tree) {
    NSDictionary *children = tree[@"children"];
    if (children) {
        Check(mkdirat(parent,name.fileSystemRepresentation,0700) == 0,@"Cannot create output directory");
        PXFD *d = Dir(parent,name);
        for (NSString *n in children) Write(d.fd,n,children[n]);
        Check(fchmod(d.fd,0755) == 0 && fsync(d.fd)==0,@"Cannot set directory permissions");
    } else {
        PXFD *f = [PXFD take:openat(parent,name.fileSystemRepresentation,O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW|O_CLOEXEC,0600)];
        NSData *data = tree[@"data"]; const char *p = data.bytes; NSUInteger left = data.length;
        while (left) { ssize_t n = write(f.fd,p,left); if (n<0 && errno==EINTR) continue; Check(n>0,@"Cannot write output"); p+=n; left-=n; }
        Check(fchmod(f.fd,0644 | ([tree[@"mode"] unsignedIntValue] & 0111)) == 0 && fsync(f.fd)==0,@"Cannot set file permissions");
    }
}
static NSString *Key(NSString *p) { return p.precomposedStringWithCanonicalMapping.lowercaseString; }
#ifdef PX_TESTING
static NSInteger PXFailureCountdown = -1;
void PXTestFailAfterPublish(NSInteger count) { PXFailureCountdown = count; }
#endif
static BOOL Overlap(NSString *a, NSString *b) { a=Key(a); b=Key(b); return [a isEqual:b] || [a hasPrefix:[b stringByAppendingString:@"/"]] || [b hasPrefix:[a stringByAppendingString:@"/"]]; }
static NSDictionary *Apply(NSDictionary *payload, uid_t uid, NSString *home, NSString *applications) {
    @try {
        Check(uid >= 501,@"System uid rejected");
        Check([payload isKindOfClass:NSDictionary.class] && payload.count == 2,@"Invalid payload");
        NSArray *pairs=payload[@"pairs"], *removes=payload[@"removes"];
        Check([pairs isKindOfClass:NSArray.class] && [removes isKindOfClass:NSArray.class] && pairs.count+removes.count<=512,@"Invalid or oversized batch");
        PXFD *base = AbsoluteDir(applications); Trusted(base.fd);
        for (NSString *name in Names(base.fd)) Check(![name hasPrefix:@".pluxel-helper-"], [NSString stringWithFormat:@"Unresolved helper transaction: %@/%@. Preserve its manifest and backups; manual recovery required before retry.",applications,name]);
        NSMutableArray *ops=[NSMutableArray new], *paths=[NSMutableArray new];
        NSUInteger nodes=0,bytes=0;
        for (id r in removes) { NSArray *a=Destination(r,applications); for (NSString *p in paths) Check(!Overlap(p,r),@"Overlapping removals"); [paths addObject:r]; [ops addObject:@{@"path":r,@"parts":a}]; }
        NSMutableArray *copyPaths=[NSMutableArray new];
        for (id pair in pairs) {
            Check([pair isKindOfClass:NSDictionary.class] && [pair count]==2,@"Invalid pair");
            NSString *src=pair[@"source"], *dst=pair[@"destination"]; Parts(src); NSArray *a=Destination(dst,applications);
            for (NSString *p in copyPaths) Check(!Overlap(p,dst),@"Overlapping copies");
            for (NSString *p in paths) Check([Key(p) isEqual:Key(dst)] || !Overlap(p,dst),@"Overlapping copy/removal");
            [copyPaths addObject:dst];
            NSString *prefix=[home stringByAppendingString:@"/Library/Application Support/com.cubezeero.pluxel/Pluxel/"];
            Check([src hasPrefix:prefix],@"Source is outside peer deployment staging");
            NSArray *tail=[[src substringFromIndex:prefix.length] componentsSeparatedByString:@"/"];
            Check(tail.count==2 && [tail[0] hasPrefix:@".deployment-"],@"Invalid staging path");
            NSString *uuid=[tail[0] substringFromIndex:12]; Check([[NSUUID alloc] initWithUUIDString:uuid]!=nil && uuid.length==36,@"Invalid deployment UUID");
            NSString *leaf=tail[1]; NSString *number=nil;
            if ([leaf hasPrefix:@"source-"]) number=[leaf substringFromIndex:7];
            if ([leaf hasPrefix:@"backup-"]) number=[leaf substringFromIndex:7];
            Check(number.length>0 && number.length<=10 && [number rangeOfCharacterFromSet:[[NSCharacterSet characterSetWithCharactersInString:@"0123456789"] invertedSet]].location==NSNotFound,@"Invalid staging item");
            PXFD *parent=AbsoluteDir([src stringByDeletingLastPathComponent]); struct stat ps=Stat(parent.fd); Check(ps.st_uid==uid,@"Deployment directory must belong to peer");
            NSDictionary *tree=Snapshot(parent.fd,leaf,uid,ps.st_dev,0,&nodes,&bytes,YES);
            [ops addObject:@{@"path":dst,@"parts":a,@"tree":tree}];
        }
        // Inspect every existing output tree before the first mutation.
        for (NSDictionary *op in ops) {
            PXFD *p=Parent(base,op[@"parts"],NO); if (!p) continue;
            NSString *leaf=[op[@"parts"] lastObject]; struct stat s;
            if (fstatat(p.fd,leaf.fileSystemRepresentation,&s,AT_SYMLINK_NOFOLLOW)==0) Snapshot(p.fd,leaf,uid,Stat(p.fd).st_dev,0,&nodes,&bytes,NO);
            else Check(errno==ENOENT,@"Cannot inspect destination");
        }
        if (!ops.count) return @{@"ok":@YES,@"value":NSNull.null};
        // Private, helper-owned staging on the destination volume. Materialize
        // every copy before moving any existing item. Renamed backups retain
        // permissions, xattrs and ownership without reading privileged contents.
        NSString *stageName=[@".pluxel-helper-" stringByAppendingString:NSUUID.UUID.UUIDString];
        Check(mkdirat(base.fd,stageName.fileSystemRepresentation,0700)==0,@"Cannot allocate helper staging");
        PXFD *stage=Dir(base.fd,stageName);
        SealPrivateDirectory(stage.fd);
        NSMutableArray *journal=[NSMutableArray new];
        @try {
            NSMutableArray *manifestOps=[NSMutableArray new];
            for (NSUInteger i=0;i<ops.count;i++) {
                NSDictionary *op=ops[i];
                [manifestOps addObject:@{@"destination":op[@"path"],@"parts":op[@"parts"],@"backup":[NSString stringWithFormat:@"old-%lu",(unsigned long)i],@"staged":[NSString stringWithFormat:@"new-%lu",(unsigned long)i],@"intent":op[@"tree"] ? @"replace" : @"remove"}];
            }
            NSData *manifest=[NSJSONSerialization dataWithJSONObject:@{@"version":@1,@"state":@"publication-intent",@"uid":@(uid),@"operations":manifestOps} options:NSJSONWritingPrettyPrinted error:NULL];
            Check(manifest!=nil,@"Cannot encode recovery manifest");
            Write(stage.fd,@"manifest.json",@{@"data":manifest,@"mode":@0600});
            PXFD *manifestFD=[PXFD take:openat(stage.fd,"manifest.json",O_RDONLY|O_NOFOLLOW|O_CLOEXEC)];
            Check(fchmod(manifestFD.fd,0600)==0 && fsync(manifestFD.fd)==0 && fsync(stage.fd)==0 && fsync(base.fd)==0,@"Cannot persist recovery manifest");
            NSUInteger index=0;
            for (NSDictionary *op in ops) {
                NSString *slot=[NSString stringWithFormat:@"new-%lu",(unsigned long)index++];
                if (op[@"tree"]) Write(stage.fd,slot,op[@"tree"]);
            }
            index=0;
            for (NSDictionary *op in ops) {
                NSString *slot=[NSString stringWithFormat:@"new-%lu",(unsigned long)index];
                NSString *backup=[NSString stringWithFormat:@"old-%lu",(unsigned long)index++];
                PXFD *p=Parent(base,op[@"parts"],op[@"tree"]!=nil); if (!p) continue;
                NSString *leaf=[op[@"parts"] lastObject];
                NSMutableDictionary *entry=[@{@"parent":p,@"leaf":leaf,@"backup":backup,@"saved":@NO,@"published":@NO} mutableCopy];
                [journal addObject:entry];
                if (renameat(p.fd,leaf.fileSystemRepresentation,stage.fd,backup.fileSystemRepresentation)==0) { entry[@"saved"]=@YES; Check(fsync(stage.fd)==0 && fsync(p.fd)==0,@"Cannot persist backup rename"); }
                else Check(errno==ENOENT,@"Cannot back up destination");
                if (op[@"tree"]) {
                    Check(renameat(stage.fd,slot.fileSystemRepresentation,p.fd,leaf.fileSystemRepresentation)==0,@"Cannot publish staged copy");
                    entry[@"published"]=@YES;
                    Check(fsync(p.fd)==0 && fsync(stage.fd)==0,@"Cannot persist publication");
                }
#ifdef PX_TESTING
                if (PXFailureCountdown > 0 && --PXFailureCountdown == 0) Fail(@"Injected publication failure");
#endif
            }
            NSData *committed=[NSJSONSerialization dataWithJSONObject:@{@"version":@1,@"state":@"committed",@"uid":@(uid),@"operations":manifestOps} options:NSJSONWritingPrettyPrinted error:NULL];
            Check(committed!=nil,@"Cannot encode commit receipt");
            Write(stage.fd,@"committed.json",@{@"data":committed,@"mode":@0600});
            Check(fsync(stage.fd)==0,@"Cannot persist commit receipt");
        } @catch(NSException *failure) {
            BOOL restored=YES;
            if(unlinkat(stage.fd,"committed.json",0)!=0 && errno!=ENOENT) restored=NO;
            if(fsync(stage.fd)!=0) restored=NO;
            for (NSDictionary *entry in journal.reverseObjectEnumerator) {
                PXFD *p=entry[@"parent"]; NSString *leaf=entry[@"leaf"], *backup=entry[@"backup"];
                @try {
                    if ([entry[@"published"] boolValue]) Remove(p.fd,leaf,stage.fd,uid,NO);
                    if ([entry[@"saved"] boolValue]) Check(renameat(stage.fd,backup.fileSystemRepresentation,p.fd,leaf.fileSystemRepresentation)==0,@"Cannot restore backup");
                    Check(fsync(p.fd)==0 && fsync(stage.fd)==0,@"Cannot persist rollback");
                } @catch(NSException *ignored) { (void)ignored; restored=NO; }
            }
            if (!restored) Fail([NSString stringWithFormat:@"%@; rollback incomplete; retained helper backups in %@/%@",failure.reason,applications,stageName]);
            @try {
                for(NSString *n in Names(stage.fd)) if(![n isEqual:@"manifest.json"] && ![n isEqual:@"committed.json"] && ![n isEqual:@"cleanup-journal.jsonl"]) Remove(stage.fd,n,stage.fd,uid,NO);
                Check(fsync(stage.fd)==0,@"Cannot sync transaction cleanup");
                if(unlinkat(stage.fd,"cleanup-journal.jsonl",0)!=0) Check(errno==ENOENT,@"Cannot remove cleanup journal");
                Check(unlinkat(stage.fd,"manifest.json",0)==0,@"Cannot remove recovery manifest");
                if(unlinkat(stage.fd,"committed.json",0)!=0) Check(errno==ENOENT,@"Cannot remove commit receipt");
                Check(unlinkat(base.fd,stageName.fileSystemRepresentation,AT_REMOVEDIR)==0,@"Cannot remove staging directory");
            } @catch(NSException *ignored) { (void)ignored; }
            @throw failure;
        }
        // A cleanup failure must not misreport a fully published batch as failed.
        // Residual backups remain private to root and cannot be addressed by API.
        @try {
                for(NSString *n in Names(stage.fd)) if(![n isEqual:@"manifest.json"] && ![n isEqual:@"committed.json"] && ![n isEqual:@"cleanup-journal.jsonl"]) Remove(stage.fd,n,stage.fd,uid,YES);
                Check(fsync(stage.fd)==0,@"Cannot sync transaction cleanup");
                if(unlinkat(stage.fd,"cleanup-journal.jsonl",0)!=0) Check(errno==ENOENT,@"Cannot remove cleanup journal");
                Check(unlinkat(stage.fd,"manifest.json",0)==0,@"Cannot remove recovery manifest");
                if(unlinkat(stage.fd,"committed.json",0)!=0) Check(errno==ENOENT,@"Cannot remove commit receipt");
                Check(unlinkat(base.fd,stageName.fileSystemRepresentation,AT_REMOVEDIR)==0,@"Cannot remove staging directory");
            } @catch(NSException *ignored) { (void)ignored; }
        return @{@"ok":@YES,@"value":NSNull.null};
    } @catch (NSException *e) { return @{@"ok":@NO,@"error":e.reason ?: @"Filesystem policy failure"}; }
}
NSDictionary *PXApply(NSDictionary *payload, uid_t uid) {
    @try {
        Check(uid>=501,@"System uid rejected"); struct passwd pw,*result=NULL; char buf[32768];
        Check(getpwuid_r(uid,&pw,buf,sizeof(buf),&result)==0 && result && pw.pw_dir,@"Peer home unavailable");
        NSString *home=[NSString stringWithUTF8String:pw.pw_dir]; Parts(home);
        Check([home hasPrefix:@"/Users/"],@"Peer home must be canonical under /Users");
        char resolved[PATH_MAX]; Check(realpath(pw.pw_dir,resolved) && !strcmp(resolved,pw.pw_dir),@"Peer home is not canonical");
        PXFD *h=AbsoluteDir(home); Check(Stat(h.fd).st_uid==uid,@"Peer home ownership mismatch");
        return Apply(payload,uid,home,@"/Applications");
    } @catch(NSException *e) { return @{@"ok":@NO,@"error":e.reason ?: @"Peer validation failed"}; }
}
#ifdef PX_TESTING
NSDictionary *PXTestApply(NSDictionary *payload, uid_t uid, NSString *home, NSString *applications) { return Apply(payload,uid,home,applications); }
#endif
