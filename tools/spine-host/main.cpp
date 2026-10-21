// FigCraft Spine host (docs/spine.md): poses a Spine skeleton with the official spine-cpp runtime
// in a process of its own, so a damaged file can only end this process, never the app.
//
// Built once per Spine version (SPINE_HOST_VERSION 42 or 43) against that version's spine-cpp,
// which is never part of the FigCraft repository (`cargo xtask spine-host`). The only C++ in
// FigCraft: everything else is Rust (crates/spine/src/host.rs drives this program).
//
// Protocol (stdin binary, stdout lines; one request at a time):
//   LOAD <binary 0|1> <skeleton bytes> <atlas bytes>\n<skeleton><atlas>
//     -> OK {"version":…,"animations":[{"name":…,"duration":…}],"skins":[…],"pages":[…]}\n | ERR <message>\n
//   POSE <time> <loop 0|1> <animation bytes> <skin bytes>\n<animation><skin>   (0 bytes: none)
//     -> FRAME <batches>\n, then per batch:
//        B <blend> <pma 0|1> <vertices> <indices> <page name>\n
//        P x y (y down) …\n  U u v …\n  C argb …\n  I i …\n
//     | ERR <message>\n
//   QUIT\n

#include <spine/spine.h>
#include <spine/SkeletonRenderer.h>

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

using namespace spine;

#if SPINE_HOST_VERSION >= 43
#define SPINE_LIST Array
#define SETUP_POSE(s) (s).setupPose()
#define DEREF(p) (*(p))
#else
#define SPINE_LIST Vector
#define SETUP_POSE(s) (s).setToSetupPose()
#define DEREF(p) (p)
#endif

namespace spine {
	SpineExtension *getDefaultExtension() {
		return new DefaultSpineExtension();
	}
}

namespace {
	// Largest skeleton or atlas accepted, and the most physics steps one pose takes.
	const size_t MAX_BYTES = 64u << 20;
	const int MAX_STEPS = 1800;

	// Pages aren't loaded: a page's texture is the page itself, so draws name their page.
	class PageLoader : public TextureLoader {
	public:
		void load(AtlasPage &page, const String &) override {
			page.texture = &page;
		}
		void unload(void *) override {
		}
	};

	std::string json_string(const String &s) {
		std::string out = "\"";
		const char *b = s.buffer();
		for (size_t i = 0; b && i < s.length(); i++) {
			unsigned char c = (unsigned char) b[i];
			if (c == '"' || c == '\\') {
				out += '\\';
				out += (char) c;
			} else if (c < 0x20) {
				char buf[8];
				snprintf(buf, sizeof buf, "\\u%04x", c);
				out += buf;
			} else {
				out += (char) c;
			}
		}
		return out + "\"";
	}

	// The rest of a name: newlines would break the line protocol.
	std::string line_safe(const String &s) {
		std::string out(s.buffer() ? s.buffer() : "", s.length());
		for (char &c : out) {
			if (c == '\n' || c == '\r') c = ' ';
		}
		return out;
	}

	bool read_exact(std::vector<char> &out, size_t n) {
		out.resize(n);
		return n == 0 || fread(out.data(), 1, n, stdin) == n;
	}

	bool read_line(std::string &line) {
		line.clear();
		int c;
		while ((c = fgetc(stdin)) != EOF) {
			if (c == '\n') return true;
			if (line.size() > 4096) return false;
			line += (char) c;
		}
		return false;
	}

	void err(const std::string &msg) {
		std::string m = msg;
		for (char &c : m) {
			if (c == '\n' || c == '\r') c = ' ';
		}
		printf("ERR %s\n", m.c_str());
		fflush(stdout);
	}

	struct Loaded {
		Atlas *atlas = nullptr;
		SkeletonData *data = nullptr;
		AnimationStateData *stateData = nullptr;
		std::vector<char> atlasText;
	};

	void info(Loaded &l) {
		std::string out = "OK {\"version\":" + json_string(l.data->getVersion()) + ",\"animations\":[";
		SPINE_LIST<Animation *> &anims = l.data->getAnimations();
		for (size_t i = 0; i < anims.size(); i++) {
			char buf[64];
			float d = anims[i]->getDuration();
			snprintf(buf, sizeof buf, "%.9g", std::isfinite(d) ? d : 0.0f);
			out += (i ? ",{\"name\":" : "{\"name\":") + json_string(anims[i]->getName()) + ",\"duration\":" + buf + "}";
		}
		out += "],\"skins\":[";
		SPINE_LIST<Skin *> &skins = l.data->getSkins();
		for (size_t i = 0; i < skins.size(); i++) {
			out += (i ? "," : "") + json_string(skins[i]->getName());
		}
		out += "],\"pages\":[";
		SPINE_LIST<AtlasPage *> &pages = l.atlas->getPages();
		for (size_t i = 0; i < pages.size(); i++) {
			out += (i ? "," : "") + json_string(pages[i]->name);
		}
		out += "]}";
		printf("%s\n", out.c_str());
		fflush(stdout);
	}

	bool load(Loaded &l, const std::string &line) {
		int binary = 0;
		unsigned long long skelLen = 0, atlasLen = 0;
		if (sscanf(line.c_str(), "LOAD %d %llu %llu", &binary, &skelLen, &atlasLen) != 3 || skelLen > MAX_BYTES || atlasLen > MAX_BYTES) {
			err("bad LOAD");
			return false;
		}
		std::vector<char> skel;
		if (!read_exact(skel, (size_t) skelLen) || !read_exact(l.atlasText, (size_t) atlasLen)) return false;
		static PageLoader loader;
		l.atlas = new Atlas(l.atlasText.data(), (int) l.atlasText.size(), "", &loader, true);
		if (l.atlas->getPages().size() == 0) {
			err("the atlas has no pages");
			return true;
		}
		if (binary) {
			SkeletonBinary reader(DEREF(l.atlas));
			l.data = reader.readSkeletonData((const unsigned char *) skel.data(), (int) skel.size());
			if (!l.data) err(std::string("invalid skeleton: ") + reader.getError().buffer());
		} else {
			skel.push_back(0);
			SkeletonJson reader(DEREF(l.atlas));
			l.data = reader.readSkeletonData(skel.data());
			if (!l.data) err(std::string("invalid skeleton: ") + reader.getError().buffer());
		}
		if (!l.data) return true;
		l.stateData = new AnimationStateData(DEREF(l.data));
		info(l);
		return true;
	}

	void pose(Loaded &l, const std::string &line) {
		double time = 0;
		int loop = 0;
		unsigned long animLen = 0, skinLen = 0;
		if (sscanf(line.c_str(), "POSE %lf %d %lu %lu", &time, &loop, &animLen, &skinLen) != 4 || animLen > 4096 || skinLen > 4096) {
			err("bad POSE");
			return;
		}
		std::vector<char> anim, skin;
		if (!read_exact(anim, animLen) || !read_exact(skin, skinLen)) exit(0);
		if (!l.data) {
			err("no skeleton loaded");
			return;
		}
		if (!std::isfinite(time) || time < 0) time = 0;
		Skeleton skeleton(DEREF(l.data));
		if (skinLen) {
			String name(std::string(skin.begin(), skin.end()).c_str());
			if (!l.data->findSkin(name)) {
				err("no such skin");
				return;
			}
			skeleton.setSkin(name);
		}
		SETUP_POSE(skeleton);
		AnimationState state(DEREF(l.stateData));
		Animation *animation = nullptr;
		if (animLen) {
			String name(std::string(anim.begin(), anim.end()).c_str());
			animation = l.data->findAnimation(name);
			if (!animation) {
				err("no such animation");
				return;
			}
			state.setAnimation(0, DEREF(animation), loop != 0);
		}
		// Physics is simulated in steps from the start, so a pose is a function of its time.
		int steps = time > 0 ? (int) std::ceil(time * 30.0) : 0;
		if (steps > MAX_STEPS) steps = MAX_STEPS;
		float dt = steps ? (float) (time / steps) : 0.0f;
		state.apply(skeleton);
		skeleton.updateWorldTransform(Physics_Reset);
		for (int i = 0; i < steps; i++) {
			state.update(dt);
			state.apply(skeleton);
			skeleton.update(dt);
			skeleton.updateWorldTransform(Physics_Update);
		}
		SkeletonRenderer renderer;
		RenderCommand *cmd = renderer.render(skeleton);
		int n = 0;
		for (RenderCommand *c = cmd; c; c = c->next) n++;
		std::string out = "FRAME " + std::to_string(n) + "\n";
		char buf[64];
		for (RenderCommand *c = cmd; c; c = c->next) {
			AtlasPage *page = (AtlasPage *) c->texture;
			snprintf(buf, sizeof buf, "B %d %d %d %d ", (int) c->blendMode, page && page->pma ? 1 : 0, c->numVertices, c->numIndices);
			out += buf;
			out += page ? line_safe(page->name) : std::string();
			out += "\nP";
			for (int i = 0; i < c->numVertices * 2; i++) {
				snprintf(buf, sizeof buf, " %.7g", c->positions[i]);
				out += buf;
			}
			out += "\nU";
			for (int i = 0; i < c->numVertices * 2; i++) {
				snprintf(buf, sizeof buf, " %.7g", c->uvs[i]);
				out += buf;
			}
			out += "\nC";
			for (int i = 0; i < c->numVertices; i++) {
				snprintf(buf, sizeof buf, " %u", (unsigned) c->colors[i]);
				out += buf;
			}
			out += "\nI";
			for (int i = 0; i < c->numIndices; i++) {
				snprintf(buf, sizeof buf, " %u", (unsigned) c->indices[i]);
				out += buf;
			}
			out += "\n";
		}
		fwrite(out.data(), 1, out.size(), stdout);
		fflush(stdout);
	}
}

int main() {
#ifdef _WIN32
	_setmode(_fileno(stdin), _O_BINARY);
	_setmode(_fileno(stdout), _O_BINARY);
#endif
	// FigCraft's y grows down (4.2 defaults to y up, 4.3 to y down).
	Bone::setYDown(true);
	Loaded l;
	std::string line;
	while (read_line(line)) {
		if (line.rfind("LOAD ", 0) == 0) {
			if (l.data || l.atlas) {
				err("already loaded");
				continue;
			}
			if (!load(l, line)) return 0;
		} else if (line.rfind("POSE ", 0) == 0) {
			pose(l, line);
		} else if (line == "QUIT") {
			return 0;
		} else {
			err("unknown request");
		}
	}
	return 0;
}
