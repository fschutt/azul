# Copy target/codegen/Azul.pm and libazul here, then:
#   cpanm --installdeps . && perl main.pl
use strict;
use warnings;
use lib '.';
use AzulStyles;

my $style_btn = AzulStyles::style_btn();
print "style_btn: ", scalar(@$style_btn), " properties\n";
