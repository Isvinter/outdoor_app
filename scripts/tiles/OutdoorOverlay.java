import com.onthegomap.planetiler.FeatureCollector;
import com.onthegomap.planetiler.FeatureMerge;
import com.onthegomap.planetiler.Planetiler;
import com.onthegomap.planetiler.Profile;
import com.onthegomap.planetiler.VectorTile;
import com.onthegomap.planetiler.config.Arguments;
import com.onthegomap.planetiler.reader.SourceFeature;
import com.onthegomap.planetiler.reader.osm.OsmElement;
import com.onthegomap.planetiler.reader.osm.OsmReader;
import com.onthegomap.planetiler.reader.osm.OsmRelationInfo;
import java.nio.file.Path;
import java.util.List;
import java.util.Set;

/**
 * Planetiler profile for the outdoor overlay: everything a topo map for hiking,
 * cycling and ski touring needs that the OpenMapTiles base map does not carry.
 *
 * Layers:
 *   trail  lines   paths and tracks with difficulty and surface attributes
 *   route  lines   signposted hiking, cycling and MTB routes (from relations)
 *   piste  lines   ski pistes and marked ski routes
 *   poi    points  peaks, saddles, huts, shelters, springs, viewpoints
 *
 * Run through scripts/build-region.sh, or directly:
 *   java -cp planetiler.jar scripts/tiles/OutdoorOverlay.java \
 *     --osm-path=region.osm.pbf --output=outdoor.pmtiles --bounds=W,S,E,N
 */
public class OutdoorOverlay implements Profile {

  private static final Set<String> TRAIL_HIGHWAYS =
    Set.of("path", "footway", "track", "cycleway", "bridleway", "steps", "via_ferrata");

  private record Route(
    @Override long id, String kind, String network, String name, String ref, String symbol, String colour
  ) implements OsmRelationInfo {}

  @Override
  public List<OsmRelationInfo> preprocessOsmRelation(OsmElement.Relation relation) {
    if (!relation.hasTag("type", "route")) {
      return null;
    }
    String kind = switch (relation.getString("route", "")) {
      case "hiking", "foot" -> "hiking";
      case "bicycle" -> "bicycle";
      case "mtb" -> "mtb";
      default -> null;
    };
    if (kind == null) {
      return null;
    }
    // iwn/icn = international, nwn/ncn = national, rwn/rcn = regional, lwn/lcn = local
    String network = switch (relation.getString("network", "")) {
      case "iwn", "icn" -> "international";
      case "nwn", "ncn" -> "national";
      case "rwn", "rcn" -> "regional";
      default -> "local";
    };
    return List.of(new Route(relation.id(), kind, network, relation.getString("name"),
      relation.getString("ref"), relation.getString("osmc:symbol"), relation.getString("colour")));
  }

  @Override
  public void processFeature(SourceFeature feature, FeatureCollector features) {
    if (feature.canBeLine()) {
      // Ferry crossings belong to many long-distance routes but are not trails.
      boolean ferry = feature.hasTag("route", "ferry");
      for (var member : ferry ? List.<OsmReader.RelationMember<Route>>of() : feature.relationInfo(Route.class, true)) {
        Route route = member.relation();
        features.line("route")
          .setAttr("kind", route.kind)
          .setAttr("network", route.network)
          .setAttr("name", route.name)
          .setAttr("ref", route.ref)
          .setAttr("osmc_symbol", route.symbol)
          .setAttr("colour", route.colour)
          .setMinZoom(switch (route.network) {
            case "international", "national" -> 7;
            case "regional" -> 9;
            default -> 11;
          })
          .setMinPixelSize(0);
      }

      String highway = feature.getString("highway", "");
      if (TRAIL_HIGHWAYS.contains(highway)) {
        features.line("trail")
          .setAttr("kind", highway)
          .setAttr("name", feature.getString("name"))
          .setAttr("sac_scale", feature.getString("sac_scale"))
          .setAttr("mtb_scale", leadingInt(feature.getString("mtb:scale")))
          .setAttr("mtb_scale_uphill", leadingInt(feature.getString("mtb:scale:uphill")))
          .setAttr("tracktype", feature.getString("tracktype"))
          .setAttr("surface", feature.getString("surface"))
          .setAttr("trail_visibility", feature.getString("trail_visibility"))
          .setAttr("bicycle", feature.getString("bicycle"))
          .setAttr("foot", feature.getString("foot"))
          .setAttr("access", feature.getString("access"))
          .setMinZoom(highway.equals("track") || highway.equals("cycleway") ? 11 : 12)
          .setMinPixelSize(0);
      }

      String piste = feature.getString("piste:type");
      if (piste != null) {
        features.line("piste")
          .setAttr("kind", piste)
          .setAttr("difficulty", feature.getString("piste:difficulty"))
          .setAttr("grooming", feature.getString("piste:grooming"))
          .setAttr("name", feature.getString("piste:name", feature.getString("name")))
          .setMinZoom(11)
          .setMinPixelSize(0);
      }
    }

    if (feature.isPoint()) {
      String kind = poiKind(feature);
      if (kind != null) {
        features.point("poi")
          .setAttr("kind", kind)
          .setAttr("name", feature.getString("name"))
          .setAttr("ele", leadingInt(feature.getString("ele")))
          .setMinZoom(kind.equals("peak") || kind.equals("alpine_hut") ? 10 : 12);
      }
    }
  }

  private static String poiKind(SourceFeature feature) {
    if (feature.hasTag("natural", "peak", "saddle", "spring")) {
      return feature.getString("natural");
    }
    if (feature.hasTag("mountain_pass", "yes")) {
      return "saddle";
    }
    if (feature.hasTag("tourism", "alpine_hut", "wilderness_hut", "viewpoint")) {
      return feature.getString("tourism");
    }
    if (feature.hasTag("amenity", "shelter")) {
      return "shelter";
    }
    return null;
  }

  /** "2", "2+" or "1650 m" become 2, 2 and 1650; anything else is dropped. */
  private static Integer leadingInt(String value) {
    if (value == null) {
      return null;
    }
    var matcher = java.util.regex.Pattern.compile("^\\s*(-?\\d+)").matcher(value);
    return matcher.find() ? Integer.valueOf(matcher.group(1)) : null;
  }

  @Override
  public List<VectorTile.Feature> postProcessLayerFeatures(String layer, int zoom, List<VectorTile.Feature> items) {
    // Join the many short OSM ways into continuous lines per set of attributes.
    return layer.equals("poi") ? items : FeatureMerge.mergeLineStrings(items, 0.5, 0.1, 4);
  }

  @Override
  public String name() {
    return "Outdoor overlay";
  }

  @Override
  public String description() {
    return "Trails, signposted routes, pistes and mountain points of interest";
  }

  @Override
  public boolean isOverlay() {
    return true;
  }

  @Override
  public String attribution() {
    return "<a href=\"https://www.openstreetmap.org/copyright\" target=\"_blank\">&copy; OpenStreetMap contributors</a>";
  }

  public static void main(String[] args) throws Exception {
    Arguments arguments = Arguments.fromArgsOrConfigFile(args);
    Path osm = Path.of(arguments.getString("osm_path", "OSM extract (.osm.pbf) to read"));
    Path output = Path.of(arguments.getString("output", "tile archive to write", "outdoor.pmtiles"));
    Planetiler.create(arguments)
      .setProfile(new OutdoorOverlay())
      .addOsmSource("osm", osm)
      .overwriteOutput(output)
      .run();
  }
}
